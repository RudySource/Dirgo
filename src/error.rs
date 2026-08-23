use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum DirgoError {
    #[error("Dirgo could not read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Dirgo configuration is invalid: {0}")]
    Config(String),
    #[error("Dirgo could not open its database: {0}")]
    Database(#[from] redb::DatabaseError),
    #[error("Dirgo could not complete a database transaction: {0}")]
    Transaction(#[from] redb::TransactionError),
    #[error("Dirgo could not access a database table: {0}")]
    Table(#[from] redb::TableError),
    #[error("Dirgo could not read a database record: {0}")]
    Storage(#[from] redb::StorageError),
    #[error("Dirgo could not commit a database update: {0}")]
    Commit(#[from] redb::CommitError),
    #[error("Dirgo data is invalid: {0}")]
    Data(#[from] serde_json::Error),
    #[error("another Dirgo index refresh is already running")]
    RefreshBusy,
    #[error("no directory matched {0:?}")]
    NoMatch(String),
    #[error("the query is ambiguous; choose one of the listed candidates")]
    Ambiguous,
    #[error("bookmark {0:?} does not exist")]
    BookmarkMissing(String),
    #[error("bookmark name {0:?} is invalid; use letters, numbers, '.', '_' or '-'")]
    InvalidBookmark(String),
    #[error("paths containing a newline cannot cross the shell integration boundary")]
    NewlinePath,
    #[error("{0}")]
    User(String),
}

impl DirgoError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

pub type Result<T> = std::result::Result<T, DirgoError>;
