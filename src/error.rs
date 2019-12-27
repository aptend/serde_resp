use std::convert::From;
use std::fmt::{self, Display};
use std::io;

use serde::{de, ser};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Message(String),
    Io(io::Error),
    Eof,
    ExpectedBoolean,
    ExpectedArray,
    ExpectedDollarSign,
    ExpectedStarSign,
    ExpectedMoreBulkString,
    ExpectedNone,
    ExpectedMoreContent,
    MismatchedName,
    MismatchedLengthHint,
    BadLengthHint,
    BadNumContent,
    UnbalancedCRLF,
    ExpectedLF,
    TrailingBytes,
}

impl ser::Error for Error {
    fn custom<T: Display>(msg: T) -> Self {
        Error::Message(msg.to_string())
    }
}

impl de::Error for Error {
    fn custom<T: Display>(msg: T) -> Self {
        Error::Message(msg.to_string())
    }
}

impl Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::Message(ref msg) => formatter.write_str(msg),
            Error::Io(ref e) => Display::fmt(e, formatter),
            Error::Eof => write!(formatter, "unexpected end of input"),
            Error::ExpectedBoolean => write!(formatter, "expected boolean"),
            Error::ExpectedArray => write!(formatter, "expected array"),
            Error::ExpectedDollarSign => write!(formatter, "expected $ sign"),
            Error::ExpectedStarSign => write!(formatter, "expected * sign"),
            Error::ExpectedMoreBulkString => write!(formatter, "expected more bulk string"),
            Error::ExpectedNone => write!(formatter, "expected none"),
            Error::ExpectedLF => write!(formatter, "expected LF"),
            Error::ExpectedMoreContent => write!(formatter, "expected more data"),
            Error::MismatchedName => write!(formatter, "mismatched name"),
            Error::MismatchedLengthHint => write!(formatter, "mismathced length hint"),
            Error::BadLengthHint => write!(formatter, "bad length hint"),
            Error::BadNumContent => write!(formatter, "bad number content"),
            Error::UnbalancedCRLF => write!(formatter, "unbalanced CRLF"),
            Error::TrailingBytes => write!(formatter, "trailing bytes"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match *self {
            Error::Io(ref inner) => Some(inner),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        if err.kind() == io::ErrorKind::UnexpectedEof {
            Error::Eof
        } else {
            Error::Io(err)
        }
    }
}
