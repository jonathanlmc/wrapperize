use std::{borrow::Cow, str::FromStr};

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

    pub fn to_bash_export_line(&self) -> anyhow::Result<String> {
        let name = shlex::try_quote(&self.name).context("failed to escape name")?;
        let value = shlex::try_quote(&self.value).context("failed to escape value")?;

        Ok(format!("export {name}={value}\n"))
    }

    pub fn into_owned(self) -> Variable<'static> {
        Variable {
            name: self.name.clone().into_owned().into(),
            value: self.value.clone().into_owned().into(),
        }
    }
}

impl<'a> TryFrom<&'a str> for Variable<'a> {
    type Error = anyhow::Error;

    fn try_from(s: &'a str) -> Result<Self, Self::Error> {
        let (name, value) = s.split_once('=').context("missing '=' separator")?;

        Ok(Self {
            name: name.into(),
            value: value.into(),
        })
    }
}

impl FromStr for Variable<'_> {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let env_var = Variable::try_from(s)?;
        Ok(env_var.into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod variable {
        use super::*;

        #[test]
        fn to_bash_export_line_succeeds() {
            let env = Variable::new("TEST", "value");

            assert_eq!(
                env.to_bash_export_line().expect("expected success"),
                "export TEST=value\n"
            )
        }

        #[test]
        fn bash_export_lines_are_escaped() {
            let env = Variable::new(
                // this is not a valid env name, but having it get escaped will make bash catch it instead of creating
                // multiple separate env vars
                "TEST NAME",
                "value with spaces",
            );

            assert_eq!(
                env.to_bash_export_line().expect("expected success"),
                "export 'TEST NAME'='value with spaces'\n"
            )
        }

        #[test]
        fn parse_from_str_succeeds() {
            let env = Variable::try_from("ENV=value").expect("env parsing should succeed");
            assert_eq!(env, Variable::new("ENV", "value"));
        }

        #[test]
        fn parse_from_str_without_sep_fails() {
            assert!(Variable::try_from("ENVvalue").is_err());
        }
    }
}
