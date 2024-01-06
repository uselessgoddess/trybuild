#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/main.src");
    t.pass("tests/ui/sample.src");
    t.pass("tests/ui/ifs.src");
}
