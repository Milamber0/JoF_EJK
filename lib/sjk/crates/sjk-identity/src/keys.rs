//! The player's identity key: an Ed25519 private key kept in one small file.
//!
//! The file is the only copy of the identity. It is created once, never
//! overwritten, and never sent anywhere; only the public half goes to the hub.
//! A file that exists but cannot be read is reported, not replaced, because
//! replacing it would silently end that identity.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};
use std::fmt;
use std::io::Write;
use std::path::Path;

/// First line of the key file; changes with an incompatible format.
const MAGIC: &str = "SJK-IDENTITY-1";

/// Why the key could not be loaded or stored.
#[derive(Debug)]
pub enum KeyError {
    /// The file exists but is not a key file this version understands.
    Corrupt(String),
    /// The file could not be read or written.
    Io(std::io::Error),
    /// The system has no random numbers to make a key from.
    NoRandom(String),
}

impl fmt::Display for KeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Corrupt(why) => write!(
                f,
                "the identity file is damaged ({why}); it was left as it is"
            ),
            Self::Io(error) => write!(f, "identity file: {error}"),
            Self::NoRandom(why) => write!(f, "no system randomness: {why}"),
        }
    }
}

impl std::error::Error for KeyError {}

impl From<std::io::Error> for KeyError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// A player's signing key.
#[derive(Clone)]
pub struct Identity {
    signing: SigningKey,
}

impl fmt::Debug for Identity {
    // Never prints the private key.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Identity({})", self.key_id())
    }
}

/// `count` random bytes from the operating system.
pub(crate) fn random_bytes<const N: usize>() -> Result<[u8; N], KeyError> {
    let mut bytes = [0_u8; N];
    getrandom::fill(&mut bytes).map_err(|error| KeyError::NoRandom(error.to_string()))?;
    Ok(bytes)
}

impl Identity {
    /// The key made from a 32-byte seed.
    pub fn from_seed(seed: [u8; 32]) -> Self {
        Self {
            signing: SigningKey::from_bytes(&seed),
        }
    }

    /// A new random key.
    pub fn generate() -> Result<Self, KeyError> {
        Ok(Self::from_seed(random_bytes()?))
    }

    /// The key stored at `path`, or a new one written there if there is no file.
    pub fn load_or_create(path: &Path) -> Result<Self, KeyError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let identity = Self::generate()?;
                match identity.create_file(path) {
                    Ok(()) => Ok(identity),
                    // Another instance made it between the read and the write.
                    Err(KeyError::Io(error))
                        if error.kind() == std::io::ErrorKind::AlreadyExists =>
                    {
                        Self::parse(&std::fs::read_to_string(path)?)
                    }
                    Err(error) => Err(error),
                }
            }
            Err(error) => Err(error.into()),
        }
    }

    /// The text of a key file.
    pub fn to_file_text(&self) -> String {
        format!("{MAGIC}\n{}\n", B64.encode(self.signing.to_bytes()))
    }

    /// Read the text of a key file (a backup the player restores).
    pub fn parse(text: &str) -> Result<Self, KeyError> {
        let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
        if lines.next() != Some(MAGIC) {
            return Err(KeyError::Corrupt("unknown format".to_owned()));
        }
        let seed = lines
            .next()
            .and_then(|line| B64.decode(line).ok())
            .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
            .ok_or_else(|| KeyError::Corrupt("the key is not 32 bytes".to_owned()))?;
        if lines.next().is_some() {
            return Err(KeyError::Corrupt(
                "unexpected text after the key".to_owned(),
            ));
        }
        Ok(Self::from_seed(seed))
    }

    /// Write the key to a file that must not exist yet, readable by its owner only
    /// where the system has such permissions.
    fn create_file(&self, path: &Path) -> Result<(), KeyError> {
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path)?;
        file.write_all(self.to_file_text().as_bytes())?;
        file.sync_all()?;
        Ok(())
    }

    /// The public key, 32 bytes.
    pub fn public_key(&self) -> [u8; 32] {
        self.signing.verifying_key().to_bytes()
    }

    /// The public key as the hub writes it (unpadded base64url).
    pub fn public_key_text(&self) -> String {
        B64.encode(self.public_key())
    }

    /// The first 16 hex digits of the public key's SHA-256: what people see and
    /// the hub's `/v1/profile/<key_id>` takes.
    pub fn key_id(&self) -> String {
        Sha256::digest(self.public_key())
            .iter()
            .take(8)
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    /// The signature of `message`, unpadded base64url.
    pub(crate) fn sign_text(&self, message: &str) -> String {
        B64.encode(self.signing.sign(message.as_bytes()).to_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_is_created_once_and_read_back() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("sub").join("identity.key");
        let first = Identity::load_or_create(&path).unwrap();
        let second = Identity::load_or_create(&path).unwrap();
        assert_eq!(first.public_key(), second.public_key());
        assert!(std::fs::read_to_string(&path).unwrap().starts_with(MAGIC));
    }

    #[test]
    fn a_damaged_file_is_reported_and_kept() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("identity.key");
        std::fs::write(&path, "garbage").unwrap();
        assert!(matches!(
            Identity::load_or_create(&path),
            Err(KeyError::Corrupt(_))
        ));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "garbage");
    }

    #[test]
    fn the_file_text_round_trips_and_rejects_extras() {
        let identity = Identity::from_seed([3; 32]);
        let text = identity.to_file_text();
        assert_eq!(
            Identity::parse(&text).unwrap().public_key(),
            identity.public_key()
        );
        assert!(Identity::parse(&format!("{text}more\n")).is_err());
        assert!(Identity::parse("SJK-IDENTITY-1\nAAAA\n").is_err());
        assert!(Identity::parse("").is_err());
    }

    #[test]
    fn debug_never_shows_the_private_key() {
        let identity = Identity::from_seed([3; 32]);
        let shown = format!("{identity:?}");
        assert!(shown.contains(&identity.key_id()));
        assert!(!shown.contains(&B64.encode([3_u8; 32])));
    }

    #[test]
    fn generated_keys_differ() {
        assert_ne!(
            Identity::generate().unwrap().public_key(),
            Identity::generate().unwrap().public_key()
        );
    }
}
