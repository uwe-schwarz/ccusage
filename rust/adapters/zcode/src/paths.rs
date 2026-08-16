use std::{collections::HashSet, env, path::PathBuf};

use crate::Result;

pub(super) const ZCODE_HOME_ENV: &str = "ZCODE_HOME";
pub(super) const ZCODE_DB_RELATIVE_PATH: &str = "cli/db/db.sqlite";

pub(super) fn zcode_db_paths() -> Result<Vec<PathBuf>> {
    let overridden = env::var(ZCODE_HOME_ENV)
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    // An unset or effectively empty override falls back to the default home,
    // matching how the Grok adapter treats its root variable.
    let homes = if overridden.is_empty() {
        let home =
            crate::home::home_dir().ok_or_else(|| crate::cli_error("home directory is not set"))?;
        vec![home.join(".zcode")]
    } else {
        overridden
    };
    let mut seen = HashSet::new();
    Ok(homes
        .into_iter()
        .map(|home| home.join(ZCODE_DB_RELATIVE_PATH))
        .filter(|path| path.is_file())
        .filter(|path| seen.insert(path.clone()))
        .collect())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use ccusage_test_support::{EnvVarGuard, EnvVarsGuard, fs_fixture};

    #[test]
    fn discovers_database_under_zcode_home() {
        let fixture = fs_fixture!({
            "cli/db/db.sqlite": "",
        });
        let _env = EnvVarGuard::set(ZCODE_HOME_ENV, fixture.root());

        let paths = zcode_db_paths().unwrap();

        assert_eq!(paths.len(), 1);
        assert!(paths[0].ends_with(Path::new(ZCODE_DB_RELATIVE_PATH)));
    }

    #[test]
    fn empty_zcode_home_falls_back_to_the_default_home() {
        let default = fs_fixture!({});
        let db = default.path(".zcode/cli/db/db.sqlite");
        std::fs::create_dir_all(db.parent().unwrap()).unwrap();
        std::fs::write(&db, "").unwrap();
        // The default home resolves through HOME, so pin it to the fixture
        // alongside the empty override; one guard covers both variables.
        let _guard = EnvVarsGuard::set_many([
            ("HOME", Some(default.root().as_os_str().to_owned())),
            (ZCODE_HOME_ENV, Some(std::ffi::OsString::from(" , "))),
        ]);

        let paths = zcode_db_paths().unwrap();

        assert_eq!(paths.len(), 1);
        assert!(paths[0].starts_with(default.root()));
    }

    #[test]
    fn skips_homes_without_a_database() {
        let with_db = fs_fixture!({
            "cli/db/db.sqlite": "",
        });
        let without_db = fs_fixture!({});
        let _env = EnvVarGuard::set(
            ZCODE_HOME_ENV,
            format!(
                "{},{}",
                without_db.root().display(),
                with_db.root().display()
            ),
        );

        let paths = zcode_db_paths().unwrap();

        assert_eq!(paths.len(), 1);
    }
}
