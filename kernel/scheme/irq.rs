use core::{mem, str};

use arch::interrupt::irq::{ACKS, COUNTS, acknowledge};
use syscall::error::*;
use syscall::scheme::Scheme;

pub struct IrqScheme;

impl Scheme for IrqScheme {
    fn open(&self, path: &[u8], _flags: usize) -> Result<usize> {
        let path_str = str::from_utf8(path).or(Err(Error::new(ENOENT)))?;

        let id = path_str.parse::<usize>().or(Err(Error::new(ENOENT)))?;

        if id < COUNTS.lock().len() {
            Ok(id)
        } else {
            Err(Error::new(ENOENT))
        }
    }

    fn dup(&self, resource: usize) -> Result<usize> {
        Ok(resource)
    }

    fn read(&self, resource: usize, buffer: &mut [u8]) -> Result<usize> {
        // Ensures that the length of the buffer is larger than the size of a usize
        if buffer.len() >= mem::size_of::<usize>() {
            let ack = ACKS.lock()[resource];
            let current = COUNTS.lock()[resource];
            if ack != current {
                // Safe if the length of the buffer is larger than the size of a usize
                assert!(buffer.len() >= mem::size_of::<usize>());
                unsafe { *(buffer.as_mut_ptr() as *mut usize) = current; }
                Ok(mem::size_of::<usize>())
            } else {
                Ok(0)
            }
        } else {
            Err(Error::new(EINVAL))
        }
    }

    fn write(&self, resource: usize, buffer: &[u8]) -> Result<usize> {
        if buffer.len() >= mem::size_of::<usize>() {
            assert!(buffer.len() >= mem::size_of::<usize>());
            let ack = unsafe { *(buffer.as_ptr() as *const usize) };
            let current = COUNTS.lock()[resource];
            if ack == current {
                ACKS.lock()[resource] = ack;
                unsafe { acknowledge(resource); }
                Ok(mem::size_of::<usize>())
            } else {
                Ok(0)
            }
        } else {
            Err(Error::new(EINVAL))
        }
    }

    fn fsync(&self, _resource: usize) -> Result<usize> {
        Ok(0)
    }

    fn close(&self, _resource: usize) -> Result<usize> {
        Ok(0)
    }
}
