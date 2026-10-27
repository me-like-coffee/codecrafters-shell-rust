use nom::{
    IResult, Parser,
    branch::alt,
    bytes::complete::{is_not, take_while},
    character::complete::{anychar, char, multispace0, multispace1, one_of, satisfy},
    combinator::{all_consuming, map, recognize},
    multi::{many0, many1, separated_list0},
    sequence::{delimited, pair, preceded},
};
use std::collections::HashMap;

#[derive(Debug, Clone)]
enum Part {
    Lit(String),
    Var(String),
}

fn lit(s: impl Into<String>) -> Vec<Part> {
    vec![Part::Lit(s.into())]
}

/// Parses a shell identifier: one letter or underscore, followed by any
/// number of letters, digits, or underscores (`[A-Za-z_][A-Za-z0-9_]*`).
/// Returns the matched name as a slice of the input. Shared by `$name`
/// and `${name}` so both forms accept exactly the same names.
fn parse_ident(input: &str) -> IResult<&str, &str> {
    recognize(pair(
        satisfy(|c| c.is_ascii_alphabetic() || c == '_'),
        take_while(|c: char| c.is_ascii_alphanumeric() || c == '_'),
    ))
    .parse(input)
}

/// `$name`, where name is [A-Za-z_][A-Za-z0-9_]*
fn parse_var(input: &str) -> IResult<&str, Vec<Part>> {
    map(
        preceded(
            char('$'),
            alt((
                delimited(char('{'), parse_ident, char('}')),
                parse_ident,
            )),
        ),
        |name: &str| vec![Part::Var(name.to_string())],
    )
    .parse(input)
}

/// A `$` not followed by an identifier stays literal.
fn parse_lone_dollar(input: &str) -> IResult<&str, Vec<Part>> {
    map(char('$'), |_| lit("$")).parse(input)
}

/// Parses content inside single quotes: '...'
/// Everything inside is strictly literal until the closing quote.
fn parse_single_quoted(input: &str) -> IResult<&str, Vec<Part>> {
    delimited(
        char('\''),
        map(take_while(|c| c != '\''), |s: &str| lit(s)),
        char('\''),
    )
    .parse(input)
}

/// Parses content inside double quotes: "..."
/// Within double quotes, \ only escapes: ", \, $, `, and newline.
fn parse_double_quoted(input: &str) -> IResult<&str, Vec<Part>> {
    delimited(
        char('"'),
        map(
            many0(alt((
                map(pair(char('\\'), one_of("\"\\$`\n")), |(_, c)| lit(c)),
                map(pair(char('\\'), anychar), |(s, c)| {
                    lit(format!("{}{}", s, c))
                }),
                parse_var,
                parse_lone_dollar,
                map(is_not("\"\\$"), |s: &str| lit(s)),
            ))),
            |v| v.into_iter().flatten().collect(),
        ),
        char('"'),
    )
    .parse(input)
}

/// Parses a backslash followed by any character outside of quotes.
fn parse_escaped_outside_quotes(input: &str) -> IResult<&str, Vec<Part>> {
    preceded(char('\\'), map(anychar, lit)).parse(input)
}

/// Parses a plain string fragment (no quotes, no whitespace, no backslash)
fn parse_plain(input: &str) -> IResult<&str, Vec<Part>> {
    map(is_not(" \t\r\n'\"\\$"), |s: &str| lit(s)).parse(input)
}

/// A "word" is one or more fragments concatenated together.
/// Fragments can be single-quoted, double-quoted, escaped, or plain.
fn parse_word(input: &str) -> IResult<&str, Vec<Part>> {
    map(
        many1(alt((
            parse_single_quoted,
            parse_double_quoted,
            parse_escaped_outside_quotes,
            parse_var,
            parse_lone_dollar,
            parse_plain,
        ))),
        |v| v.into_iter().flatten().collect(),
    )
    .parse(input)
}

/// Parses the entire command line into a list of words.
fn parse_line(input: &str) -> IResult<&str, Vec<Vec<Part>>> {
    delimited(
        multispace0,
        separated_list0(multispace1, parse_word),
        multispace0,
    )
    .parse(input)
}

/// The public entry point for the parser.
pub fn tokenize(input: &str, vars: &HashMap<String, String>) -> anyhow::Result<Vec<String>> {
    let (_, words) = all_consuming(parse_line)
        .parse(input)
        .map_err(|e| anyhow::anyhow!("Invalid syntax: {}", e))?;
    Ok(words
        .into_iter()
        .map(|parts| {
            parts
                .into_iter()
                .map(|p| match p {
                    Part::Lit(s) => s,
                    Part::Var(name) => vars
                        .get(&name)
                        .cloned()
                        .or_else(|| std::env::var(&name).ok())
                        .unwrap_or_default(),
                })
                .collect::<String>()
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tk(s: &str) -> anyhow::Result<Vec<String>> {
        tokenize(s, &HashMap::new())
    }

    #[test]
    fn test_single_quotes() {
        // Preserves spaces literally
        assert_eq!(tk("'hello    world'").unwrap(), vec!["hello    world"]);
        // Literal inside single
        assert_eq!(tk("'shell\"s test'").unwrap(), vec!["shell\"s test"]);
    }

    #[test]
    fn test_plain_spaces() {
        // Collapses unquoted spaces
        assert_eq!(tk("hello    world").unwrap(), vec!["hello", "world"]);
    }

    #[test]
    fn test_concatenation() {
        // Quoted strings next to each other
        assert_eq!(tk("'hello''world'").unwrap(), vec!["helloworld"]);
        assert_eq!(tk("\"hello\"\"world\"").unwrap(), vec!["helloworld"]);
        assert_eq!(tk("hello''world").unwrap(), vec!["helloworld"]);
    }

    #[test]
    fn test_double_quotes() {
        // Preserves spaces
        assert_eq!(tk("\"hello    world\"").unwrap(), vec!["hello    world"]);
        // Single quote inside double is literal
        assert_eq!(tk("\"shell's test\"").unwrap(), vec!["shell's test"]);
    }

    #[test]
    fn test_backslash_outside_quotes() {
        assert_eq!(
            tk("echo three\\ \\ \\ spaces").unwrap(),
            vec!["echo", "three   spaces"]
        );
        assert_eq!(
            tk("echo before\\     after").unwrap(),
            vec!["echo", "before ", "after"]
        );
        assert_eq!(
            tk("echo test\\nexample").unwrap(),
            vec!["echo", "testnexample"]
        );
        assert_eq!(
            tk("echo hello\\\\world").unwrap(),
            vec!["echo", "hello\\world"]
        );
        assert_eq!(tk("echo \\'hello\\'").unwrap(), vec!["echo", "'hello'"]);
    }

    #[test]
    fn test_backslash_inside_double_quotes() {
        // Special escapes
        assert_eq!(
            tk("\"A \\\\ escapes itself\"").unwrap(),
            vec!["A \\ escapes itself"]
        );
        assert_eq!(
            tk("\"A \\\" inside double quotes\"").unwrap(),
            vec!["A \" inside double quotes"]
        );
        // Literal backslash (n is not special)
        assert_eq!(tk("\"hello\\nworld\"").unwrap(), vec!["hello\\nworld"]);
    }

    #[test]
    fn test_mixed_complex() {
        // 'hello' + space (escaped) + "world"
        assert_eq!(tk("'hello'\\ \"world\"").unwrap(), vec!["hello world"]);
    }
}
