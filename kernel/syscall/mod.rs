/// ! Syscall handlers

extern crate syscall;

pub use self::syscall::{data, error, flag, number, scheme};

pub use self::fs::*;
pub use self::process::*;
pub use self::validate::*;

use self::data::Stat;
use self::error::{Error, Result, ENOSYS};
use self::number::*;

/// Filesystem syscalls
pub mod fs;

/// Process syscalls
pub mod process;

/// Validate input
pub mod validate;

#[no_mangle]
pub extern "C" fn syscall(a: usize,
                          b: usize,
                          c: usize,
                          d: usize,
                          e: usize,
                          f: usize,
                          stack: usize)
                          -> usize {
    #[inline(always)]
    fn inner(a: usize,
             b: usize,
             c: usize,
             d: usize,
             e: usize,
             _f: usize,
             stack: usize)
             -> Result<usize> {
        match a {
            SYS_EXIT => exit(b),
            SYS_READ => read(b, validate_slice_mut(c as *mut u8, d)?),
            SYS_WRITE => write(b, validate_slice(c as *const u8, d)?),
            SYS_OPEN => open(validate_slice(b as *const u8, c)?, d),
            SYS_CLOSE => close(b),
            SYS_WAITPID => waitpid(b, c, d),
            SYS_UNLINK => unlink(validate_slice(b as *const u8, c)?),
            SYS_EXECVE => {
                exec(validate_slice(b as *const u8, c)?,
                     validate_slice(d as *const [usize; 2], e)?)
            }
            SYS_CHDIR => chdir(validate_slice(b as *const u8, c)?),
            SYS_LSEEK => lseek(b, c, d),
            SYS_GETPID => getpid(),
            SYS_FSTAT => fstat(b, &mut validate_slice_mut(c as *mut Stat, 1)?[0]),
            SYS_MKDIR => mkdir(validate_slice(b as *const u8, c)?, d),
            SYS_RMDIR => rmdir(validate_slice(b as *const u8, c)?),
            SYS_DUP => dup(b),
            SYS_BRK => brk(b),
            SYS_FTRUNCATE => ftruncate(b, c),
            SYS_IOPL => iopl(b),
            SYS_FSYNC => fsync(b),
            SYS_CLONE => clone(b, stack),
            SYS_YIELD => sched_yield(),
            SYS_GETCWD => getcwd(validate_slice_mut(b as *mut u8, c)?),
            SYS_FEVENT => fevent(b, c),
            SYS_FPATH => fpath(b, validate_slice_mut(c as *mut u8, d)?),
            SYS_PHYSALLOC => physalloc(b),
            SYS_PHYSFREE => physfree(b, c),
            SYS_PHYSMAP => physmap(b, c, d),
            SYS_PHYSUNMAP => physunmap(b),
            SYS_VIRTTOPHYS => virttophys(b),
            _ => {
                println!("Unknown syscall {}", a);
                Err(Error::new(ENOSYS))
            }
        }
    }

    let result = inner(a, b, c, d, e, f, stack);

    if let Err(ref err) = result {
        println!("{}, {}, {}, {}: {}", a, b, c, d, err);
    }

    Error::mux(result)
}
