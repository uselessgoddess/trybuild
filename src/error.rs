use {
    glob::{GlobError, PatternError},
    std::{
        ffi::OsString,
        fmt::{self, Display},
        io,
        path::PathBuf,
    },
};

#[derive(Debug)]
pub enum Error {
    Cargo(io::Error),
    CargoFail,
    GetManifest(PathBuf, Box<Error>),
    Glob(GlobError),
    Io(io::Error),
    Metadata(serde_json::Error),
    Mismatch,
    NoWorkspaceManifest,
    Open(PathBuf, io::Error),
    Pattern(PatternError),
    ProjectDir,
    ReadStderr(io::Error),
    RunFailed,
    ShouldNotHaveCompiled,
    Toml(basic_toml::Error),
    UpdateVar(OsString),
    WriteStderr(io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::Error::*;

        match self {
            Cargo(e) => write!(f, "failed to execute cargo: {}", e),
            CargoFail => write!(f, "cargo reported an error"),
            GetManifest(path, e) => write!(f, "failed to read manifest {}: {}", path.display(), e),
            Glob(e) => write!(f, "{}", e),
            Io(e) => write!(f, "{}", e),
            Metadata(e) => write!(f, "failed to read cargo metadata: {}", e),
            Mismatch => write!(f, "compiler error does not match expected error"),
            NoWorkspaceManifest => write!(
                f,
                "Cargo.toml uses edition.workspace=true, \
                but no edition found in workspace's manifest"
            ),
            Open(path, e) => write!(f, "{}: {}", path.display(), e),
            Pattern(e) => write!(f, "{}", e),
            ProjectDir => write!(f, "failed to determine name of project dir"),
            ReadStderr(e) => write!(f, "failed to read stderr file: {}", e),
            RunFailed => write!(f, "execution of the test case was unsuccessful"),
            ShouldNotHaveCompiled => {
                write!(f, "expected test case to fail to compile, but it succeeded")
            }
            Toml(e) => write!(f, "{}", e),
            UpdateVar(var) => {
                write!(f, "unrecognized value of TRYBUILD: {:?}", var.to_string_lossy(),)
            }
            WriteStderr(e) => write!(f, "failed to write stderr file: {}", e),
        }
    }
}

impl Error {
    pub fn already_printed(&self) -> bool {
        use self::Error::*;

        matches!(self, CargoFail | Mismatch | RunFailed | ShouldNotHaveCompiled)
    }
}

impl From<GlobError> for Error {
    fn from(err: GlobError) -> Self {
        Error::Glob(err)
    }
}

impl From<PatternError> for Error {
    fn from(err: PatternError) -> Self {
        Error::Pattern(err)
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::Io(err)
    }
}

impl From<basic_toml::Error> for Error {
    fn from(err: basic_toml::Error) -> Self {
        Error::Toml(err)
    }
}
