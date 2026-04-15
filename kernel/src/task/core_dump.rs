//! Minimal `ET_CORE` ELF with a single `PT_NOTE` (signal number, PID, user PC/SP).

use alloc::format;
use alloc::{vec, vec::Vec};

use axerrno::{AxError, AxResult};
use axfs::OpenOptions;
use axhal::uspace::UserContext;
use axio::prelude::*;
use axio::Write;
use linux_raw_sys::general::{AT_FDCWD, RLIMIT_CORE};

use super::Thread;
use crate::file::with_fs;
use crate::syscall::{sys_getegid, sys_geteuid};

const ELFMAG: [u8; 4] = [0x7f, b'E', b'L', b'F'];
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const EV_CURRENT: u8 = 1;
const ET_CORE: u16 = 4;
const EM_RISCV: u16 = 243;
const PT_NOTE: u32 = 4;
const NT_PRSTATUS: u32 = 1;

fn push_bytes(buf: &mut Vec<u8>, bytes: &[u8]) {
    buf.extend_from_slice(bytes);
}

fn push_u16(buf: &mut Vec<u8>, v: u16) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn push_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn push_u64(buf: &mut Vec<u8>, v: u64) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn build_min_core_image(pid: u32, signo: u32, ip: u64, sp: u64) -> Vec<u8> {
    let descsz: u32 = 256;
    let namesz: u32 = 4;
    let note_hdr_and_name = (12 + namesz as usize + 3) & !3;
    let note_total = note_hdr_and_name + descsz as usize;
    let ehdr_sz = 64usize;
    let phdr_sz = 56usize;
    let note_off = ehdr_sz + phdr_sz;

    let mut desc = vec![0u8; descsz as usize];
    desc[0..4].copy_from_slice(&signo.to_le_bytes());
    desc[4..8].copy_from_slice(&pid.to_le_bytes());
    desc[8..16].copy_from_slice(&ip.to_le_bytes());
    desc[16..24].copy_from_slice(&sp.to_le_bytes());

    let mut buf = Vec::with_capacity(note_off + note_total);
    push_bytes(&mut buf, &ELFMAG);
    buf.push(ELFCLASS64);
    buf.push(ELFDATA2LSB);
    buf.push(EV_CURRENT);
    push_bytes(&mut buf, &[0u8; 9]);
    push_u16(&mut buf, ET_CORE);
    push_u16(&mut buf, EM_RISCV);
    push_u32(&mut buf, 1);
    push_u64(&mut buf, 0);
    push_u64(&mut buf, ehdr_sz as u64);
    push_u64(&mut buf, 0);
    push_u32(&mut buf, 0);
    push_u16(&mut buf, ehdr_sz as u16);
    push_u16(&mut buf, phdr_sz as u16);
    push_u16(&mut buf, 1);
    push_u16(&mut buf, 0);
    push_u16(&mut buf, 0);
    push_u16(&mut buf, 0);

    push_u32(&mut buf, PT_NOTE);
    push_u32(&mut buf, 0);
    push_u64(&mut buf, note_off as u64);
    push_u64(&mut buf, 0);
    push_u64(&mut buf, 0);
    push_u64(&mut buf, note_total as u64);
    push_u64(&mut buf, note_total as u64);
    push_u64(&mut buf, 1);

    push_u32(&mut buf, namesz);
    push_u32(&mut buf, descsz);
    push_u32(&mut buf, NT_PRSTATUS);
    push_bytes(&mut buf, b"CORE");
    push_bytes(&mut buf, &desc);

    buf
}

fn write_min_elf_core_inner(thr: &Thread, uctx: &UserContext, signo: u32) -> AxResult<()> {
    let limit = thr.proc_data.rlim.read()[RLIMIT_CORE].current;
    if limit == 0 {
        return Ok(());
    }

    let ip = uctx.ip() as u64;
    let sp = uctx.sp() as u64;
    let pid_u32 = thr.proc_data.proc.pid();
    let buf = build_min_core_image(pid_u32, signo, ip, sp);

    if buf.len() as u64 > limit {
        return Err(AxError::InvalidInput);
    }

    let mode = 0o600u32 & !thr.proc_data.umask();
    let uid = sys_geteuid()? as u32;
    let gid = sys_getegid()? as u32;
    let path = format!("core.{pid_u32}");

    let mut opts = OpenOptions::new();
    opts.write(true)
        .truncate(true)
        .create(true)
        .mode(mode)
        .user(uid, gid);

    with_fs(AT_FDCWD as _, |fs| {
        let file = opts
            .open(fs, path.as_str())
            .map_err(|_| AxError::Io)?
            .into_file()
            .map_err(|_| AxError::InvalidInput)?;
        let mut fref = &file;
        Write::write_all(&mut fref, &buf).map_err(|_| AxError::Io)?;
        Write::flush(&mut fref).map_err(|_| AxError::Io)?;
        Ok(())
    })
}

/// Writes `core.<pid>` in the current working directory when `RLIMIT_CORE` allows.
pub(crate) fn write_min_elf_core(thr: &Thread, uctx: &UserContext, signo: u32) {
    if let Err(e) = write_min_elf_core_inner(thr, uctx, signo) {
        warn!("core dump failed: {e:?}");
    }
}
