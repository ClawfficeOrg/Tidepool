//! Printing.
//!
//! `--json` is always the SDK's own type: the printer serialises the value the
//! SDK returned, so a script reads exactly the contract shape. Everything else
//! is a short, line-oriented table for a person.

use std::io::Write;

use serde::Serialize;

use crate::error::{CliError, Result};

/// One output stream, in whichever mode the command line asked for.
pub struct Printer<'a> {
    json: bool,
    out: &'a mut dyn Write,
}

impl<'a> Printer<'a> {
    /// Build a printer that writes JSON when `json` is set.
    pub fn new(json: bool, out: &'a mut dyn Write) -> Self {
        Self { json, out }
    }

    /// Whether this printer emits JSON.
    pub fn is_json(&self) -> bool {
        self.json
    }

    /// Print one SDK value as pretty JSON.
    pub fn json<T: Serialize>(&mut self, value: &T) -> Result<()> {
        let rendered = serde_json::to_string_pretty(value)?;
        self.line(rendered)
    }

    /// Print one SDK value as a single JSON line, for a followed stream.
    pub fn json_line<T: Serialize>(&mut self, value: &T) -> Result<()> {
        let rendered = serde_json::to_string(value)?;
        self.line(rendered)
    }

    /// Print one value as JSON, or `human`'s text when not in JSON mode.
    pub fn emit<T: Serialize>(
        &mut self,
        value: &T,
        human: impl FnOnce(&T) -> String,
    ) -> Result<()> {
        if self.json {
            self.json(value)
        } else {
            self.line(human(value))
        }
    }

    /// Print one line of text.
    pub fn line(&mut self, line: impl AsRef<str>) -> Result<()> {
        writeln!(self.out, "{}", line.as_ref()).map_err(CliError::Output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_mode_serialises_the_type_itself() {
        let mut buffer = Vec::new();
        let mut printer = Printer::new(true, &mut buffer);
        printer
            .emit(&vec!["a", "b"], |values| values.join(","))
            .expect("emit");
        let rendered = String::from_utf8(buffer).expect("utf8");
        assert_eq!(rendered.trim(), "[\n  \"a\",\n  \"b\"\n]");
    }

    #[test]
    fn text_mode_uses_the_human_renderer() {
        let mut buffer = Vec::new();
        let mut printer = Printer::new(false, &mut buffer);
        printer
            .emit(&2_u8, |value| format!("{value} things"))
            .expect("emit");
        assert_eq!(String::from_utf8(buffer).expect("utf8"), "2 things\n");
    }

    #[test]
    fn a_followed_stream_is_one_json_object_per_line() {
        let mut buffer = Vec::new();
        let mut printer = Printer::new(true, &mut buffer);
        printer
            .json_line(&serde_json::json!({"sequence": 1}))
            .expect("line");
        printer
            .json_line(&serde_json::json!({"sequence": 2}))
            .expect("line");
        let rendered = String::from_utf8(buffer).expect("utf8");
        let lines: Vec<&str> = rendered.lines().collect();
        assert_eq!(lines, vec![r#"{"sequence":1}"#, r#"{"sequence":2}"#]);
    }
}
