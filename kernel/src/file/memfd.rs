use alloc::{borrow::Cow, format, string::String};
use core::task::Context;

use axerrno::AxResult;
use axpoll::{IoEvents, Pollable};

use super::{File, FileLike, IoDst, IoSrc, Kstat};

/// Maximum characters taken from `memfd_create(2)` / `memfd_secret(2)` name for display.
const MEMFD_NAME_MAX: usize = 200;

fn sanitize_memfd_name(s: &str) -> String {
    let s = s.trim();
    if s.is_empty() {
        return String::from("anonymous");
    }
    s.chars()
        .take(MEMFD_NAME_MAX)
        .map(|c| match c {
            '/' | '\\' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect()
}

/// [`memfd_create`] / [`memfd_secret`] backing is still a real file under tmpfs (`/tmp/memfd-*`);
/// [`FileLike::path`] returns a Linux-style **`/memfd:<name>`** string so `/proc/self/fd/N`
/// readlink can reflect the user-supplied display name.
pub struct MemfdCreatedFile {
    file: File,
    link_path: String,
}

impl MemfdCreatedFile {
    pub fn new(file: File, user_name: &str) -> Self {
        let n = sanitize_memfd_name(user_name);
        Self {
            file,
            link_path: format!("/memfd:{n}"),
        }
    }

    pub(crate) fn inner_file(&self) -> &File {
        &self.file
    }
}

impl FileLike for MemfdCreatedFile {
    fn read(&self, dst: &mut IoDst) -> AxResult<usize> {
        self.file.read(dst)
    }

    fn write(&self, src: &mut IoSrc) -> AxResult<usize> {
        self.file.write(src)
    }

    fn stat(&self) -> AxResult<Kstat> {
        self.file.stat()
    }

    fn path(&self) -> Cow<'_, str> {
        Cow::Owned(self.link_path.clone())
    }

    fn ioctl(&self, cmd: u32, arg: usize) -> AxResult<usize> {
        self.file.ioctl(cmd, arg)
    }

    fn nonblocking(&self) -> bool {
        self.file.nonblocking()
    }

    fn set_nonblocking(&self, flag: bool) -> AxResult {
        self.file.set_nonblocking(flag)
    }
}

impl Pollable for MemfdCreatedFile {
    fn poll(&self) -> IoEvents {
        self.file.poll()
    }

    fn register(&self, context: &mut Context<'_>, events: IoEvents) {
        self.file.register(context, events);
    }
}
