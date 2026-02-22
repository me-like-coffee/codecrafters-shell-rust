use nom::{
    IResult, Parser,
    branch::alt,
    bytes::complete::{is_not, take_while},
    character::complete::{anychar, char, multispace0, multispace1, one_of},
    combinator::{all_consuming, map},
    multi::{many0, many1, separated_list0},
    sequence::{delimited, pair, preceded},
};

/// Parses content inside single quotes: '...'
/// Everything inside is strictly literal until the closing quote.
fn parse_single_quoted(input: &str) -> IResult<&str, String> {
    delimited(
        char('\''),
        map(take_while(|c| c != '\''), String::from),
        char('\''),
    )
    .parse(input)
}

/// Parses content inside double quotes: "..."
/// Within double quotes, \ only escapes: ", \, $, `, and newline.
fn parse_double_quoted(input: &str) -> IResult<&str, String> {
    delimited(
        char('"'),
        map(
            many0(alt((
                // Handle special escapes: \" or \\ (and eventually $, `, \n)
                // We consume both but only return the second character
                map(pair(char('\\'), one_of("\"\\$`\n")), |(_, c)| c.to_string()),
                // Handle a backslash followed by any other character
                // The backslash remains literal
                map(pair(char('\\'), anychar), |(slash, c)| {
                    format!("{}{}", slash, c)
                }),
                // Handle any character that is NOT a quote or a backslash
                map(is_not("\"\\"), String::from),
            ))),
            |parts| parts.join(""),
        ),
        char('"'),
    )
    .parse(input)
}

/// Parses a backslash followed by any character outside of quotes.
fn parse_escaped_outside_quotes(input: &str) -> IResult<&str, String> {
    preceded(char('\\'), map(anychar, String::from)).parse(input)
}

/// Parses a plain string fragment (no quotes, no whitespace, no backslash)
fn parse_plain(input: &str) -> IResult<&str, String> {
    map(is_not(" \t\r\n'\"\\"), String::from).parse(input)
}

/// A "word" is one or more fragments concatenated together.
/// Fragments can be single-quoted, double-quoted, escaped, or plain.
fn parse_word(input: &str) -> IResult<&str, String> {
    map(
        many1(alt((
            parse_single_quoted,
            parse_double_quoted,
            parse_escaped_outside_quotes,
            parse_plain,
        ))),
        |parts| parts.join(""),
    )
    .parse(input)
}

/// Parses the entire command line into a list of words.
fn parse_line(input: &str) -> IResult<&str, Vec<String>> {
    delimited(
        multispace0,
        separated_list0(multispace1, parse_word),
        multispace0,
    )
    .parse(input)
}

/// The public entry point for the parser.
pub fn tokenize(input: &str) -> anyhow::Result<Vec<String>> {
    let (_, tokens) = all_consuming(parse_line)
        .parse(input)
        .map_err(|e| anyhow::anyhow!("Invalid syntax: {}", e))?;
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_quotes() {
        // Preserves spaces literally
        assert_eq!(
            tokenize("'hello    world'").unwrap(),
            vec!["hello    world"]
        );
        // Literal inside single
        assert_eq!(tokenize("'shell\"s test'").unwrap(), vec!["shell\"s test"]);
    }

    #[test]
    fn test_plain_spaces() {
        // Collapses unquoted spaces
        assert_eq!(tokenize("hello    world").unwrap(), vec!["hello", "world"]);
    }

    #[test]
    fn test_concatenation() {
        // Quoted strings next to each other
        assert_eq!(tokenize("'hello''world'").unwrap(), vec!["helloworld"]);
        assert_eq!(tokenize("\"hello\"\"world\"").unwrap(), vec!["helloworld"]);
        assert_eq!(tokenize("hello''world").unwrap(), vec!["helloworld"]);
    }

    #[test]
    fn test_double_quotes() {
        // Preserves spaces
        assert_eq!(
            tokenize("\"hello    world\"").unwrap(),
            vec!["hello    world"]
        );
        // Single quote inside double is literal
        assert_eq!(tokenize("\"shell's test\"").unwrap(), vec!["shell's test"]);
    }

    #[test]
    fn test_backslash_outside_quotes() {
        assert_eq!(
            tokenize("echo three\\ \\ \\ spaces").unwrap(),
            vec!["echo", "three   spaces"]
        );
        assert_eq!(
            tokenize("echo before\\     after").unwrap(),
            vec!["echo", "before ", "after"]
        );
        assert_eq!(
            tokenize("echo test\\nexample").unwrap(),
            vec!["echo", "testnexample"]
        );
        assert_eq!(
            tokenize("echo hello\\\\world").unwrap(),
            vec!["echo", "hello\\world"]
        );
        assert_eq!(
            tokenize("echo \\'hello\\'").unwrap(),
            vec!["echo", "'hello'"]
        );
    }

    #[test]
    fn test_backslash_inside_double_quotes() {
        // Special escapes
        assert_eq!(
            tokenize("\"A \\\\ escapes itself\"").unwrap(),
            vec!["A \\ escapes itself"]
        );
        assert_eq!(
            tokenize("\"A \\\" inside double quotes\"").unwrap(),
            vec!["A \" inside double quotes"]
        );
        // Literal backslash (n is not special)
        assert_eq!(
            tokenize("\"hello\\nworld\"").unwrap(),
            vec!["hello\\nworld"]
        );
    }

    #[test]
    fn test_mixed_complex() {
        // 'hello' + space (escaped) + "world"
        assert_eq!(
            tokenize("'hello'\\ \"world\"").unwrap(),
            vec!["hello world"]
        );
    }
}
