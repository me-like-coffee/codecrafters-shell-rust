use nom::{
    IResult, Parser,
    branch::alt,
    bytes::complete::{is_not, take_while},
    character::complete::{char, multispace0, multispace1},
    combinator::{all_consuming, map},
    multi::{many1, separated_list0},
    sequence::delimited,
};

/// Parses content inside single quotes: '...'
/// Everything inside is literal.
fn parse_single_quoted(input: &str) -> IResult<&str, String> {
    delimited(
        char('\''),
        map(take_while(|c| c != '\''), String::from),
        char('\''),
    )
    .parse(input)
}

/// Parses content inside double quotes: "..."
/// For now, handles literals (preserves spaces and single quotes).
fn parse_double_quoted(input: &str) -> IResult<&str, String> {
    delimited(
        char('"'),
        map(take_while(|c| c != '"'), String::from),
        char('"'),
    )
    .parse(input)
}

/// Parses a plain string fragment (no quotes, no whitespace)
fn parse_plain(input: &str) -> IResult<&str, String> {
    // We stop at whitespace, single quotes, or double quotes
    map(is_not(" \t\r\n'\""), String::from).parse(input)
}

/// A "word" is one or more fragments (single-quoted, double-quoted, or plain)
/// concatenated together. Handles "hello"'world' -> helloworld
fn parse_word(input: &str) -> IResult<&str, String> {
    map(
        many1(alt((parse_single_quoted, parse_double_quoted, parse_plain))),
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
    fn test_double_quotes_preserve_spaces() {
        let input = "echo \"hello    world\"";
        let tokens = tokenize(input).unwrap();
        assert_eq!(tokens, vec!["echo", "hello    world"]);
    }

    #[test]
    fn test_concatenation() {
        let input = "echo \"hello\"\"world\"";
        let tokens = tokenize(input).unwrap();
        assert_eq!(tokens, vec!["echo", "helloworld"]);
    }

    #[test]
    fn test_separate_arguments() {
        let input = "echo \"hello\" \"world\"";
        let tokens = tokenize(input).unwrap();
        assert_eq!(tokens, vec!["echo", "hello", "world"]);
    }

    #[test]
    fn test_single_inside_double() {
        let input = "echo \"shell's test\"";
        let tokens = tokenize(input).unwrap();
        assert_eq!(tokens, vec!["echo", "shell's test"]);
    }

    #[test]
    fn test_mixed_concatenation() {
        let input = "echo 'single'\"double\"plain";
        let tokens = tokenize(input).unwrap();
        assert_eq!(tokens, vec!["echo", "singledoubleplain"]);
    }
}
