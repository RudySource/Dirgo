use crate::{DirgoError, Result, shell::Shell};

use super::PaletteAction;

const FRAME_VERSION: &str = "DGP1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteResultKind {
    Navigate,
    Insert,
}

impl PaletteResultKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Navigate => "navigate",
            Self::Insert => "insert",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteResultFrame {
    kind: PaletteResultKind,
    payload: String,
}

impl PaletteResultFrame {
    pub fn from_action(action: &PaletteAction, shell: Shell) -> Result<Option<Self>> {
        let (kind, payload) = match action {
            PaletteAction::Navigate { path } => (
                PaletteResultKind::Navigate,
                path.to_str().ok_or(DirgoError::NonUtf8Path)?.to_owned(),
            ),
            PaletteAction::Insert { text } => (PaletteResultKind::Insert, text.clone()),
            PaletteAction::InsertCommand { program, args } => (
                PaletteResultKind::Insert,
                render_command(shell, program, args)?,
            ),
            PaletteAction::Open { .. }
            | PaletteAction::CopyPath { .. }
            | PaletteAction::OpenEditor { .. } => return Ok(None),
        };
        validate_payload(&payload)?;
        Ok(Some(Self { kind, payload }))
    }

    pub fn kind(&self) -> PaletteResultKind {
        self.kind
    }

    pub fn payload(&self) -> &str {
        &self.payload
    }

    pub fn encode(&self) -> String {
        format!("{FRAME_VERSION} {}\n{}", self.kind.as_str(), self.payload)
    }
}

fn render_command(shell: Shell, program: &str, args: &[String]) -> Result<String> {
    let mut words = Vec::with_capacity(args.len() + 1);
    words.push(quote_word(shell, program)?);
    for arg in args {
        words.push(quote_word(shell, arg)?);
    }
    Ok(words.join(" "))
}

fn quote_word(shell: Shell, value: &str) -> Result<String> {
    validate_payload(value)?;
    let quoted = match shell {
        Shell::Zsh | Shell::Bash => format!("'{}'", value.replace('\'', "'\\''")),
        Shell::Fish => format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'")),
        Shell::PowerShell => format!("'{}'", value.replace('\'', "''")),
    };
    Ok(quoted)
}

fn validate_payload(value: &str) -> Result<()> {
    if value.contains(['\r', '\n']) || value.chars().any(|character| character.is_control()) {
        return Err(DirgoError::User(
            "Palette actions cannot contain line breaks or terminal control characters".into(),
        ));
    }
    Ok(())
}
