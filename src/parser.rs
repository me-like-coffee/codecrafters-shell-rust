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
fn parse_single_quoted(input: &str) -> IResult<&str, String> {
    delimited(
        char('\''),
        map(take_while(|c| c != '\''), String::from),
        char('\''),
    )
    .parse(input)
}

/// Parses a plain string fragment (no quotes, no whitespace)
fn parse_plain(input: &str) -> IResult<&str, String> {
    map(is_not(" \t\r\n'"), String::from).parse(input)
}

/// A "word" is one or more fragments (quoted or plain) concatenated together.
fn parse_word(input: &str) -> IResult<&str, String> {
    map(many1(alt((parse_single_quoted, parse_plain))), |parts| {
        parts.join("")
    })
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
/// Bridges nom's internal error type to anyhow::Result.
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
    fn test_concatenation() {
        assert_eq!(tokenize("'hello''world'").unwrap(), vec!["helloworld"]);
        assert_eq!(tokenize("hello''world").unwrap(), vec!["helloworld"]);
    }

    #[test]
    fn test_spaces() {
        assert_eq!(
            tokenize("'hello    world'").unwrap(),
            vec!["hello    world"]
        );
        assert_eq!(tokenize("hello    world").unwrap(), vec!["hello", "world"]);
    }
}
