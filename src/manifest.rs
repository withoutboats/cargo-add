use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use toml;

pub type Dependency = (String, toml::Value);

#[derive(Debug)]
/// Catch-all error for misconfigured crates.
pub struct ManifestError;

impl Error for ManifestError {
    fn description(&self) -> &str {
        "Your Cargo.toml is either missing or incorrectly structured."
    }
}

impl fmt::Display for ManifestError {
    fn fmt(&self, format: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        format.write_str(self.description())
    }
}

#[derive(Debug)]
/// Catch-all error for misconfigured crates.
pub struct ManifestPathError;

impl Error for ManifestPathError {
    fn description(&self) -> &str {
        "The manifest path you supplied was not valid."
    }
}

impl fmt::Display for ManifestPathError {
    fn fmt(&self, format: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        format.write_str(self.description())
    }
}

enum CargoFile {
    Config,
    Lock,
}

#[derive(Debug, PartialEq)]
pub struct Manifest {
    pub data: toml::Table,
}

/// If a manifest is specified, return that one, otherise perform a manifest search starting from
/// the current directory.
fn find(specified: &Option<&String>, file: CargoFile) -> Result<PathBuf, Box<Error>> {
    let file_path = specified.map(PathBuf::from);

    if let Some(path) = file_path {
        if try!(fs::metadata(&path)).is_file() {
            Ok(path)
        } else {
            Err(ManifestPathError).map_err(From::from)
        }
    } else {
        env::current_dir()
            .map_err(From::from)
            .and_then(|ref dir| search(dir, file).map_err(From::from))
    }
}

/// Search for Cargo.toml in this directory and recursively up the tree until one is found.
fn search(dir: &Path, file: CargoFile) -> Result<PathBuf, ManifestError> {
    let manifest = match file {
        CargoFile::Config => dir.join("Cargo.toml"),
        CargoFile::Lock => dir.join("Cargo.lock"),
    };

    fs::metadata(&manifest)
        .map(|_| manifest)
        .or(dir.parent().ok_or(ManifestError).and_then(|dir| search(dir, file)))
}

impl Manifest {
    pub fn from_str(input: &str) -> Result<Manifest, Box<Error>> {
        let mut parser = toml::Parser::new(&input);

        parser.parse()
              .ok_or(parser.errors.pop())
              .map_err(Option::unwrap)
              .map_err(From::from)
              .map(|data| Manifest { data: data })
    }

    pub fn find_file(path: &Option<&String>) -> Result<File, Box<Error>> {
        find(path, CargoFile::Config).and_then(|path| {
            OpenOptions::new()
                .read(true)
                .write(true)
                .open(path)
                .map_err(From::from)
        })
    }

    pub fn find_lock_file(path: &Option<&String>) -> Result<File, Box<Error>> {
        find(path, CargoFile::Lock).and_then(|path| {
            OpenOptions::new()
                .read(true)
                .write(true)
                .open(path)
                .map_err(From::from)
        })
    }

    pub fn open(path: &Option<&String>) -> Result<Manifest, Box<Error>> {
        let mut file = try!(Manifest::find_file(path));
        let mut data = String::new();
        try!(file.read_to_string(&mut data));

        Manifest::from_str(&data)
    }

    pub fn open_lock_file(path: &Option<&String>) -> Result<Manifest, Box<Error>> {
        let mut file = try!(Manifest::find_lock_file(path));
        let mut data = String::new();
        try!(file.read_to_string(&mut data));

        Manifest::from_str(&data)
    }

    /// Overwrite a file with TOML data.
    pub fn write_to_file<T: Seek + Write>(&self, file: &mut T) -> Result<(), Box<Error>> {
        try!(file.seek(SeekFrom::Start(0)));
        let mut toml = self.data.clone();

        let (proj_header, proj_data) = try!(toml.remove("package")
                                                .map(|data| ("package", data))
                                                .or_else(|| {
                                                    toml.remove("project")
                                                        .map(|data| ("project", data))
                                                })
                                                .ok_or(ManifestError));
        write!(file,
               "[{}]\n{}{}",
               proj_header,
               proj_data,
               toml::Value::Table(toml))
            .map_err(From::from)
    }

    /// Add entry to a Cargo.toml.
    fn insert_into_table(&mut self,
                         table: &str,
                         &(ref name, ref data): &Dependency)
                         -> Result<(), ManifestError> {
        let ref mut manifest = self.data;
        let entry = manifest.entry(String::from(table))
                            .or_insert(toml::Value::Table(BTreeMap::new()));
        match entry {
            &mut toml::Value::Table(ref mut deps) => {
                deps.insert(name.clone(), data.clone());
                Ok(())
            }
            _ => Err(ManifestError),
        }
    }

    /// Add entry to manifest
    pub fn add_deps(&mut self, table: &str, deps: &[Dependency]) -> Result<(), Box<Error>> {
        deps.iter()
            .map(|dep| self.insert_into_table(table, &dep))
            .collect::<Result<Vec<_>, _>>()
            .map_err(From::from)
            .map(|_| ())
    }
}
