use nom::{
    IResult, Parser,
    branch::alt,
    bytes::complete::{is_not, take_while},
    character::complete::{anychar, char, multispace0, multispace1},
    combinator::{all_consuming, map},
    multi::{many1, separated_list0},
    sequence::{delimited, preceded},
};

/// Parses content inside single quotes: '...'
fn parse_single_quoted(input: &str) -> IResult<&str, String> {
    delimited(
        char('\''),
        map(take_while(|c| c != '\''), String::from),
        char('\''),
    )
    .parse(input)
}

/// Parses content inside double quotes: "..."
fn parse_double_quoted(input: &str) -> IResult<&str, String> {
    delimited(
        char('"'),
        map(take_while(|c| c != '"'), String::from),
        char('"'),
    )
    .parse(input)
}

/// Parses a backslash followed by any character outside of quotes.
/// Returns the escaped character as a literal string.
fn parse_escaped_outside_quotes(input: &str) -> IResult<&str, String> {
    preceded(char('\\'), map(anychar, String::from)).parse(input)
}

/// Parses a plain string fragment (no quotes, no whitespace, no backslash)
fn parse_plain(input: &str) -> IResult<&str, String> {
    // We stop at whitespace, single quotes, double quotes, or backslashes
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
    fn test_backslash_spaces() {
        let input = "echo three\\ \\ \\ spaces";
        let tokens = tokenize(input).unwrap();
        assert_eq!(tokens, vec!["echo", "three   spaces"]);
    }

    #[test]
    fn test_backslash_preserves_first_space() {
        let input = "echo before\\     after";
        let tokens = tokenize(input).unwrap();
        assert_eq!(tokens, vec!["echo", "before ", "after"]);
    }

    #[test]
    fn test_backslash_no_special_meaning() {
        let input = "echo test\\nexample";
        let tokens = tokenize(input).unwrap();
        assert_eq!(tokens, vec!["echo", "testnexample"]);
    }

    #[test]
    fn test_backslash_backslash() {
        let input = "echo hello\\\\world";
        let tokens = tokenize(input).unwrap();
        assert_eq!(tokens, vec!["echo", "hello\\world"]);
    }

    #[test]
    fn test_backslash_quotes() {
        let input = "echo \\'hello\\'";
        let tokens = tokenize(input).unwrap();
        assert_eq!(tokens, vec!["echo", "'hello'"]);
    }

    #[test]
    fn test_mixed_quoted_and_escaped() {
        let input = "echo \"hello\"\\ world'!'";
        let tokens = tokenize(input).unwrap();
        assert_eq!(tokens, vec!["echo", "hello world!"]);
    }
}
