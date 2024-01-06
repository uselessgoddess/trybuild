use std::{
    borrow::Cow,
    env,
    ffi::OsString,
    io,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct Directory {
    path: PathBuf,
}

impl Directory {
    pub fn new<P: Into<PathBuf>>(path: P) -> Self {
        let mut path = path.into();
        path.push("");
        Directory { path }
    }

    pub fn current() -> io::Result<Self> {
        env::current_dir().map(Directory::new)
    }

    pub fn to_string_lossy(&self) -> Cow<str> {
        self.path.to_string_lossy()
    }

    pub fn join<P: AsRef<Path>>(&self, tail: P) -> PathBuf {
        self.path.join(tail)
    }

    pub fn parent(&self) -> Option<Self> {
        self.path.parent().map(Directory::new)
    }

    pub fn canonicalize(&self) -> io::Result<Self> {
        self.path.canonicalize().map(Directory::new)
    }
}

impl From<OsString> for Directory {
    fn from(os_string: OsString) -> Self {
        Directory::new(PathBuf::from(os_string))
    }
}

impl AsRef<Path> for Directory {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}
