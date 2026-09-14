use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub struct TestDir(PathBuf);

impl TestDir {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!(
            "envcon-regression-{}-{stamp}-{}",
            std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    pub fn path(&self) -> &Path { &self.0 }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub fn fixture_wheel(path: &Path) {
    use std::io::Write;
    let file = std::fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let entries = [
        ("envcon_fixture.py", "def main():\n    print('envcon-fixture-ok')\n"),
        ("envcon_fixture-1.0.0.dist-info/METADATA", "Metadata-Version: 2.1\nName: envcon-fixture\nVersion: 1.0.0\n"),
        ("envcon_fixture-1.0.0.dist-info/WHEEL", "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n"),
        ("envcon_fixture-1.0.0.dist-info/entry_points.txt", "[console_scripts]\nenvcon-fixture = envcon_fixture:main\n"),
    ];
    let mut record = String::new();
    for (name, contents) in entries {
        zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(contents.as_bytes()).unwrap();
        record.push_str(&format!("{name},,\n"));
    }
    let name = "envcon_fixture-1.0.0.dist-info/RECORD";
    zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
    zip.write_all(format!("{record}{name},,\n").as_bytes()).unwrap();
    zip.finish().unwrap();
}
