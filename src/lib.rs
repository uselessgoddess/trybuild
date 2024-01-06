mod diff;
mod error;
mod flock;
mod message;
mod normalize;

#[macro_use]
mod path;

#[macro_use]
mod term;
mod directory;

use {
    crate::{directory::Directory, error::Error, flock::Lock},
    std::{
        cell::RefCell,
        collections::HashMap,
        env,
        ffi::OsString,
        fs::File,
        path::{Path, PathBuf},
        thread,
    },
};

#[derive(Debug)]
pub struct TestCases {
    runner: RefCell<Runner>,
}

#[derive(Debug)]
struct Runner {
    tests: Vec<Test>,
}

#[derive(Debug)]
struct ExpandedTest {
    pub name: String,
    pub test: Test,
    pub error: Option<Error>,
    is_from_glob: bool,
}

impl ExpandedTest {
    fn run(&self, project: &Project) -> Result<Outcome> {
        self.test.run(project, &self.name)
    }
}

struct ExpandedTestSet {
    vec: Vec<ExpandedTest>,
    path_to_index: HashMap<PathBuf, usize>,
}

impl ExpandedTestSet {
    fn new() -> Self {
        ExpandedTestSet { vec: Vec::new(), path_to_index: HashMap::new() }
    }

    fn insert(&mut self, test: Test, error: Option<Error>, is_from_glob: bool) {
        if let Some(&i) = self.path_to_index.get(&test.path) {
            let prev = &mut self.vec[i];
            if prev.is_from_glob {
                prev.test.expected = test.expected;
                return;
            }
        }

        let index = self.vec.len();
        let name = format!("trybuild{:03}", index);
        self.path_to_index.insert(test.path.clone(), index);
        self.vec.push(ExpandedTest { name, test, error, is_from_glob });
    }
}

impl Runner {
    fn expand_globs(tests: &[Test]) -> Vec<ExpandedTest> {
        let mut set = ExpandedTestSet::new();

        for test in tests {
            match test.path.to_str() {
                Some(utf8) if utf8.contains('*') => match glob(utf8) {
                    Ok(paths) => {
                        let expected = test.expected;
                        for path in paths {
                            set.insert(Test { path, expected }, None, true);
                        }
                    }
                    Err(error) => set.insert(test.clone(), Some(error), false),
                },
                _ => set.insert(test.clone(), None, false),
            }
        }

        set.vec
    }

    fn filter(tests: &mut Vec<ExpandedTest>) {
        let filters = env::args_os()
            .flat_map(OsString::into_string)
            .filter_map(|mut arg| {
                const PREFIX: &str = "trybuild=";
                if arg.starts_with(PREFIX) && arg != PREFIX {
                    Some(arg.split_off(PREFIX.len()))
                } else {
                    None
                }
            })
            .collect::<Vec<String>>();

        if filters.is_empty() {
            return;
        }

        tests.retain(|t| filters.iter().any(|f| t.test.path.to_string_lossy().contains(f)));
    }
}

type Result<T, E = Error> = std::result::Result<T, E>;

fn glob(pattern: &str) -> Result<Vec<PathBuf>> {
    let mut paths = glob::glob(pattern)?
        .map(|entry| entry.map_err(Error::from))
        .collect::<Result<Vec<PathBuf>>>()?;
    paths.sort();
    Ok(paths)
}

#[derive(Clone, Debug)]
struct Test {
    path: PathBuf,
    expected: Expected,
}

struct Stderr {
    success: bool,
    stderr: Vec<u8>,
}

impl Test {
    fn run(&self, project: &Project, name: &str) -> Result<Outcome> {
        let show_expected = project.has_pass && project.has_compile_fail;
        message::begin_test(self, show_expected);
        check_exists(&self.path)?;

        let output = zxc::build_test(project, &self.path, name)?;
        let stderr = Stderr { success: false, stderr: output.stderr };
        self.check(project, name, &stderr, &String::from_utf8_lossy(&output.stdout))
    }

    fn check(
        &self,
        project: &Project,
        name: &str,
        result: &Stderr,
        build_stdout: &str,
    ) -> Result<Outcome> {
        let check = match self.expected {
            Expected::Pass => Test::check_pass,
            //Expected::CompileFail => Test::check_compile_fail,
            _ => todo!(),
        };

        check(
            self,
            project,
            name,
            result.success,
            build_stdout,
            &String::from_utf8_lossy(&result.stderr),
        )
    }

    fn check_pass(
        &self,
        project: &Project,
        name: &str,
        success: bool,
        build_stdout: &str,
        variations: &str,
    ) -> Result<Outcome> {
        if !success {
            message::failed_to_build(variations);
            return Err(Error::CargoFail);
        }

        let mut output = zxc::run_test(project, name)?;
        output.stdout.splice(..0, build_stdout.bytes());
        message::output(variations, &output);
        if output.status.success() { Ok(Outcome::Passed) } else { Err(Error::RunFailed) }
    }
}

#[derive(Copy, Clone, Debug)]
enum Expected {
    Pass,
    CompileFail,
}

impl TestCases {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        TestCases { runner: RefCell::new(Runner { tests: Vec::new() }) }
    }

    pub fn pass<P: AsRef<Path>>(&self, path: P) {
        self.runner
            .borrow_mut()
            .tests
            .push(Test { path: path.as_ref().to_owned(), expected: Expected::Pass });
    }

    pub fn compile_fail<P: AsRef<Path>>(&self, path: P) {
        self.runner
            .borrow_mut()
            .tests
            .push(Test { path: path.as_ref().to_owned(), expected: Expected::CompileFail });
    }
}

impl Drop for TestCases {
    fn drop(&mut self) {
        if !thread::panicking() {
            self.runner.borrow_mut().run();
        }
    }
}

#[derive(Debug)]
pub struct Project {
    pub dir: Directory,
    pub has_pass: bool,
    has_compile_fail: bool,
    pub keep_going: bool,
}

struct Report {
    failures: usize,
    created_wip: usize,
}

enum Outcome {
    Passed,
    CreatedWip,
}

fn check_exists(path: &Path) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    match File::open(path) {
        Ok(_) => Ok(()),
        Err(err) => Err(Error::Open(path.to_owned(), err)),
    }
}

impl Runner {
    fn prepare(&self, tests: &[ExpandedTest]) -> Result<Project> {
        let mut has_pass = false;
        let mut has_compile_fail = false;
        for e in tests {
            match e.test.expected {
                Expected::Pass => has_pass = true,
                Expected::CompileFail => has_compile_fail = true,
            }
        }

        Ok(Project {
            dir: path!(env::current_dir()? /),
            has_pass: false,
            has_compile_fail,
            keep_going: true,
        })
    }

    fn run_all(&self, project: &Project, tests: Vec<ExpandedTest>) -> Result<Report> {
        let mut report = Report { failures: 0, created_wip: 0 };

        let mut path_map = HashMap::new();
        for t in &tests {
            let src_path = project.dir.join(&t.test.path);
            path_map.insert(src_path, (&t.name, &t.test));
        }

        for mut t in tests {
            let show_expected = false;
            message::begin_test(&t.test, show_expected);

            if t.error.is_none() {
                t.error = check_exists(&t.test.path).err();
            }

            if t.error.is_none() {
                let output = zxc::build_test(project, &t.test.path, &t.name)?;

                let stderr = Stderr { success: output.status.success(), stderr: output.stderr };
                match t.test.check(project, &t.name, &stderr, "") {
                    Ok(Outcome::Passed) => {}
                    Ok(Outcome::CreatedWip) => report.created_wip += 1,
                    Err(error) => t.error = Some(error),
                }
            }

            if let Some(err) = t.error {
                report.failures += 1;
                message::test_fail(err);
            }
        }

        Ok(report)
    }

    pub fn run(&mut self) {
        let mut tests = Self::expand_globs(&self.tests);
        Self::filter(&mut tests);

        let (project, _lock) = (|| {
            let mut project = self.prepare(&tests)?;
            let lock = Lock::acquire(path!(project.dir / ".lock"))?;
            Ok((project, lock))
        })()
        .unwrap_or_else(|err| {
            message::prepare_fail(err);
            panic!("tests failed");
        });

        print!("\n\n");

        let len = tests.len();
        let mut report = Report { failures: 0, created_wip: 0 };

        if tests.is_empty() {
            message::no_tests_enabled();
        } else if project.keep_going && !project.has_pass {
            report = self.run_all(&project, tests).unwrap_or_else(|err| {
                message::test_fail(err);
                Report { failures: len, created_wip: 0 }
            })
        } else {
            for test in tests {
                match test.run(&project) {
                    Ok(Outcome::Passed) => {}
                    Ok(Outcome::CreatedWip) => report.created_wip += 1,
                    Err(err) => {
                        report.failures += 1;
                        message::test_fail(err);
                    }
                }
            }
        }

        print!("\n\n");

        if report.failures > 0 {
            panic!("{} of {} tests failed", report.failures, len);
        }
        if report.created_wip > 0 {
            panic!("successfully created new stderr files for {} test cases", report.created_wip,);
        }
    }
}

mod zxc {
    use {
        super::Result,
        crate::{error::Error, Project},
        std::{
            path::Path,
            process::{Command, Output},
        },
    };

    fn zxc() -> Command {
        Command::new("cargo").args(["run", "--package", "driver"]).output().unwrap();
        Command::new("../../target/debug/driver")
    }

    pub fn build_test(project: &Project, test: &Path, name: &str) -> Result<Output> {
        zxc()
            .arg(project.dir.join(test))
            .arg("--out-dir")
            .arg(".artifacts")
            .arg("-o")
            .arg(name)
            .output()
            .map_err(Error::Cargo)
    }

    pub fn run_test(_: &Project, test: &str) -> Result<Output> {
        Command::new(format!(".artifacts/{test}")).output().map_err(Error::Cargo)
    }
}
