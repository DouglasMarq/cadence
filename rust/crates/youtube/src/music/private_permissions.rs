use std::fs::{DirBuilder, OpenOptions};

#[cfg(unix)]
mod owner_only {
    use std::fs::{DirBuilder, OpenOptions, Permissions};
    use std::io;
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
    use std::path::Path;

    const PRIVATE_DIRECTORY_MODE: u32 = 0o700;
    const PRIVATE_FILE_MODE: u32 = 0o600;

    pub(super) fn restrict_new_directories(builder: &mut DirBuilder) {
        builder.mode(PRIVATE_DIRECTORY_MODE);
    }

    pub(in crate::music) fn restrict_directory(directory: &Path) -> io::Result<()> {
        std::fs::set_permissions(directory, Permissions::from_mode(PRIVATE_DIRECTORY_MODE))
    }

    pub(super) fn restrict_new_files(options: &mut OpenOptions) {
        options.mode(PRIVATE_FILE_MODE);
    }
}

#[cfg(windows)]
mod owner_only {
    use std::fs::{DirBuilder, OpenOptions};
    use std::io;
    use std::path::Path;

    pub(super) fn restrict_new_directories(_inherits_the_user_folder_acl: &mut DirBuilder) {}

    pub(in crate::music) fn restrict_directory(
        _inherits_the_user_folder_acl: &Path,
    ) -> io::Result<()> {
        Ok(())
    }

    pub(super) fn restrict_new_files(_inherits_the_user_folder_acl: &mut OpenOptions) {}
}

pub(super) use owner_only::restrict_directory;

pub(super) fn private_directory_builder() -> DirBuilder {
    let mut builder = DirBuilder::new();
    builder.recursive(true);
    owner_only::restrict_new_directories(&mut builder);
    builder
}

pub(super) fn private_file_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    owner_only::restrict_new_files(&mut options);
    options
}
