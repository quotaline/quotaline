//! Finds the current user's Antigravity processes and the loopback ports they listen on.

use quotaline_antigravity::local::{LocalRole, role};
use std::{collections::BTreeMap, ffi::c_void, mem, ptr, slice};
use windows_sys::{
    Wdk::System::Threading::{NtQueryInformationProcess, ProcessCommandLineInformation},
    Win32::{
        Foundation::{CloseHandle, HANDLE, LocalFree, UNICODE_STRING},
        NetworkManagement::IpHelper::{
            GetExtendedTcpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCPROW_OWNER_PID, TCP_TABLE_OWNER_PID_LISTENER,
        },
        Networking::WinSock::{AF_INET, AF_INET6},
        Security::{EqualSid, GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser},
        System::Threading::{
            GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
            QueryFullProcessImageNameW,
        },
        UI::Shell::CommandLineToArgvW,
    },
};

/// Closes a process or token handle when dropped.
struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

/// Copies a table the IP Helper fills in, growing the buffer it asks for.
fn table(family: u16) -> Vec<u32> {
    let mut size = 0u32;
    unsafe {
        GetExtendedTcpTable(
            ptr::null_mut(),
            &mut size,
            0,
            family.into(),
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        )
    };
    // u32 cells keep the rows aligned for the reads below.
    let mut buffer = vec![0u32; (size as usize).div_ceil(4) + 1];
    let status = unsafe {
        GetExtendedTcpTable(
            buffer.as_mut_ptr().cast(),
            &mut size,
            0,
            family.into(),
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        )
    };
    if status != 0 {
        buffer.clear();
    }
    buffer
}

/// Each listening process and its ports, from the IPv4 and IPv6 tables.
fn listeners() -> BTreeMap<u32, Vec<u16>> {
    let mut ports: BTreeMap<u32, Vec<u16>> = BTreeMap::new();
    let port = |raw: u32| u16::from_be(raw as u16);
    let ipv4 = table(AF_INET);
    if let Some((&count, rows)) = ipv4.split_first() {
        let rows = unsafe { slice::from_raw_parts(rows.as_ptr().cast::<MIB_TCPROW_OWNER_PID>(), count as usize) };
        for row in rows {
            ports.entry(row.dwOwningPid).or_default().push(port(row.dwLocalPort));
        }
    }
    let ipv6 = table(AF_INET6);
    if let Some((&count, rows)) = ipv6.split_first() {
        let rows = unsafe { slice::from_raw_parts(rows.as_ptr().cast::<MIB_TCP6ROW_OWNER_PID>(), count as usize) };
        for row in rows {
            ports.entry(row.dwOwningPid).or_default().push(port(row.dwLocalPort));
        }
    }
    ports
}

/// The security identifier of the user who owns `process`, as raw bytes.
fn owner(process: HANDLE) -> Option<Vec<u8>> {
    let mut token = ptr::null_mut();
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return None;
    }
    let token = Handle(token);
    let mut size = 0u32;
    unsafe { GetTokenInformation(token.0, TokenUser, ptr::null_mut(), 0, &mut size) };
    let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
    if unsafe { GetTokenInformation(token.0, TokenUser, buffer.as_mut_ptr().cast(), size, &mut size) } == 0 {
        return None;
    }
    let sid = unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    // A SID is 8 bytes plus 4 per sub-authority, whose count is its second byte.
    let length = 8 + 4 * usize::from(unsafe { *sid.cast::<u8>().add(1) });
    Some(unsafe { slice::from_raw_parts(sid.cast::<u8>(), length) }.to_vec())
}

fn same_user(left: &[u8], right: &[u8]) -> bool {
    unsafe { EqualSid(left.as_ptr() as *mut c_void, right.as_ptr() as *mut c_void) != 0 }
}

fn image(process: HANDLE) -> Option<String> {
    let mut buffer = vec![0u16; 32_768];
    let mut length = buffer.len() as u32;
    if unsafe { QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length) } == 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..length as usize]))
}

fn arguments(process: HANDLE) -> Option<Vec<String>> {
    let mut size = 0u32;
    unsafe { NtQueryInformationProcess(process, ProcessCommandLineInformation, ptr::null_mut(), 0, &mut size) };
    let mut buffer = vec![
        0u64;
        (size as usize)
            .div_ceil(8)
            .max(mem::size_of::<UNICODE_STRING>().div_ceil(8))
    ];
    let status = unsafe {
        NtQueryInformationProcess(
            process,
            ProcessCommandLineInformation,
            buffer.as_mut_ptr().cast(),
            (buffer.len() * 8) as u32,
            &mut size,
        )
    };
    if status < 0 {
        return None;
    }
    let line = unsafe { &*buffer.as_ptr().cast::<UNICODE_STRING>() };
    let mut text: Vec<u16> = unsafe { slice::from_raw_parts(line.Buffer, usize::from(line.Length) / 2) }.to_vec();
    text.push(0);
    let mut count = 0;
    let list = unsafe { CommandLineToArgvW(text.as_ptr(), &mut count) };
    if list.is_null() {
        return None;
    }
    let arguments = unsafe { slice::from_raw_parts(list, count as usize) }
        .iter()
        .map(|argument| {
            let length = (0..).take_while(|&index| unsafe { *argument.add(index) } != 0).count();
            String::from_utf16_lossy(unsafe { slice::from_raw_parts(*argument, length) })
        })
        .collect();
    unsafe { LocalFree(list.cast()) };
    Some(arguments)
}

/// Rewrites a Windows path the way the shared matcher reads Unix ones: `/` separators and no `.exe`.
fn unix_style(path: &str) -> String {
    let path = path.replace('\\', "/");
    path.strip_suffix(".exe").map(str::to_owned).unwrap_or(path)
}

pub(super) fn processes() -> Vec<(LocalRole, Vec<u16>)> {
    let Some(user) = owner(unsafe { GetCurrentProcess() }) else {
        return Vec::new();
    };
    listeners()
        .into_iter()
        .filter_map(|(pid, mut ports)| {
            let process = Handle(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) });
            if process.0.is_null() || !same_user(&owner(process.0)?, &user) {
                return None;
            }
            let path = unix_style(&image(process.0)?);
            let arguments: Vec<String> = arguments(process.0)?
                .iter()
                .map(|argument| unix_style(argument))
                .collect();
            let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
            let role = role(&path, &arguments)?;
            ports.sort_unstable();
            ports.dedup();
            Some((role, ports))
        })
        .collect()
}
