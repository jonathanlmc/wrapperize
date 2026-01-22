use std::{borrow::Cow, fmt, str::FromStr};

use anyhow::Context;

#[derive(Debug, PartialEq)]
pub struct Variable<'a> {
    pub name: Cow<'a, str>,
    pub value: Cow<'a, str>,
}

impl<'a> Variable<'a> {
    #[cfg(test)]
    pub fn new(name: impl Into<Cow<'a, str>>, value: impl Into<Cow<'a, str>>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }

    pub fn parse<'b>(value: &'b str) -> anyhow::Result<Variable<'b>> {
        let (name, value) = value.split_once('=').context("missing '=' separator")?;

        anyhow::ensure!(
            Self::is_valid_name(name),
            "invalid name for environment variable `{name}`",
        );

        Ok(Variable {
            name: name.into(),
            value: value.into(),
        })
    }

    pub fn write_bash_line(&self, mut writer: impl fmt::Write) -> fmt::Result {
        let escaped_value = self.value.as_bytes().escape_ascii().to_string();

        writeln!(
            writer,
            r#"export {name}="{value}""#,
            name = self.name,
            value = escaped_value
        )
    }

    pub fn into_owned(self) -> Variable<'static> {
        Variable {
            name: self.name.clone().into_owned().into(),
            value: self.value.clone().into_owned().into(),
        }
    }

    fn is_valid_name(name: &str) -> bool {
        if name.is_empty() {
            return false;
        }

        for (i, ch) in name.chars().enumerate() {
            if ch == '_' {
                continue;
            }

            if i == 0 && !ch.is_alphabetic() {
                return false;
            }

            if !ch.is_alphanumeric() {
                return false;
            }
        }

        true
    }
}

impl<'a> TryFrom<&'a str> for Variable<'a> {
    type Error = anyhow::Error;

    fn try_from(s: &'a str) -> Result<Self, Self::Error> {
        Self::parse(s)
    }
}

impl FromStr for Variable<'_> {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).map(|env| env.into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod variable {
        use super::*;

        #[test]
        fn is_valid_name() {
            assert_eq!(Variable::is_valid_name("HELLO"), true);
            assert_eq!(Variable::is_valid_name("hello"), true);
            assert_eq!(Variable::is_valid_name("_HELLO"), true);
            assert_eq!(Variable::is_valid_name("HELLO1"), true);
            assert_eq!(Variable::is_valid_name("HELLO1_WORLD"), true);
            assert_eq!(Variable::is_valid_name("____"), true);

            assert_eq!(Variable::is_valid_name(""), false);
            assert_eq!(Variable::is_valid_name("1HELLO"), false);
            assert_eq!(Variable::is_valid_name("@HELLO"), false);
            assert_eq!(Variable::is_valid_name("HELL@"), false);
        }

        #[test]
        fn write_bash_line() {
            let env = Variable::new("TEST", "value");

            let mut buffer = String::new();
            env.write_bash_line(&mut buffer)
                .expect("write bash line should succeed");

            assert_eq!(buffer, "export TEST=\"value\"\n")
        }

        #[test]
        fn bash_lines_are_quote_escaped() {
            let env = Variable::new("TEST", r#"value "with" quotes"#);

            let mut buffer = String::new();
            env.write_bash_line(&mut buffer)
                .expect("write bash line should succeed");

            assert_eq!(buffer, "export TEST=\"value \\\"with\\\" quotes\"\n")
        }

        #[test]
        fn parse_from_str_succeeds() {
            let env = Variable::try_from("ENV=value").expect("env parsing should succeed");
            assert_eq!(env, Variable::new("ENV", "value"));
        }

        #[test]
        fn parse_from_str_without_sep_fails() {
            assert!(Variable::parse("ENVvalue").is_err());
        }
    }
}
