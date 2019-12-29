pub mod de;
mod error;
pub mod ser;

pub use de::from_bytes;
pub use de::from_reader;
pub use ser::to_bytes;

pub use error::Error;
