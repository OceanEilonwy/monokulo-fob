//! Keeping an unlocked wallet's memory inside the process that holds it.
//!
//! Locked pages ([`crate::locked`]) keep a few keys out of swap and core dumps,
//! but an unlocked wallet leaves more behind than those: the file key copied
//! through the database layer, typed passphrases, seed words on their way to a
//! key. A crash would hand all of it to systemd-coredump, ABRT or apport, which
//! keep cores on disk for days and may offer to upload them. So a process that
//! will hold keys says, before it holds any, that it's never to be dumped.

/// Makes this process undumpable and gives it no room for a core file. Call it
/// first thing in `main`, before any key is loaded. Failing to is no error: the
/// process runs as it would have, so nothing here can stop a wallet opening.
///
/// `PR_SET_DUMPABLE` set to 0 also stops other processes of the same user from
/// attaching with ptrace or reading `/proc/<pid>/mem`, which distributions that
/// leave `kernel.yama.ptrace_scope` at 0 (Debian, Fedora) otherwise allow. Root
/// still can, and hibernation or unencrypted swap still write memory to disk:
/// that's the system's to protect.
pub fn harden_process() {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        // SAFETY: prctl with PR_SET_DUMPABLE takes a plain integer and touches
        // no memory of ours; the unused arguments are zero as the man page asks.
        unsafe {
            let _ = libc::prctl(libc::PR_SET_DUMPABLE, 0 as libc::c_ulong, 0 as libc::c_ulong, 0 as libc::c_ulong, 0 as libc::c_ulong);
        }
    }
    #[cfg(unix)]
    {
        let none = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        // SAFETY: setrlimit only reads the struct we pass, which lives across the call.
        unsafe {
            let _ = libc::setrlimit(libc::RLIMIT_CORE, &none);
        }
    }
}

/// Makes every file and folder this process creates from now on readable by
/// this user alone (umask 077). Most are made with explicit modes already, but
/// not all of them are ours to make: SQLCipher creates the wallet databases and
/// their journals, and would leave them readable by every account on the
/// machine. On Android each app's folder is private anyway, so only the desktop
/// calls this.
pub fn keep_files_private() {
    #[cfg(unix)]
    {
        // SAFETY: umask takes a plain mode and can't fail.
        unsafe {
            libc::umask(0o077);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn a_hardened_process_has_no_room_for_a_core_and_cant_be_dumped() {
        harden_process();
        let mut limit = libc::rlimit { rlim_cur: 1, rlim_max: 1 };
        // SAFETY: getrlimit writes into the struct we own.
        assert_eq!(unsafe { libc::getrlimit(libc::RLIMIT_CORE, &mut limit) }, 0);
        assert_eq!((limit.rlim_cur, limit.rlim_max), (0, 0));
        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            // SAFETY: PR_GET_DUMPABLE reads a flag and takes no pointers.
            assert_eq!(unsafe { libc::prctl(libc::PR_GET_DUMPABLE, 0, 0, 0, 0) }, 0);
        }
    }
}
