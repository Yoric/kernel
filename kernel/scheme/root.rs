use alloc::arc::Arc;
use alloc::boxed::Box;
use collections::BTreeMap;
use core::sync::atomic::{AtomicUsize, Ordering};
use spin::RwLock;

use context;
use syscall::error::*;
use syscall::scheme::Scheme;
use scheme;
use scheme::user::{UserInner, UserScheme};

pub struct RootScheme {
    next_id: AtomicUsize,
    handles: RwLock<BTreeMap<usize, Arc<UserInner>>>
}

impl RootScheme {
    pub fn new() -> RootScheme {
        RootScheme {
            next_id: AtomicUsize::new(0),
            handles: RwLock::new(BTreeMap::new())
        }
    }
}

impl Scheme for RootScheme {
    fn open(&self, path: &[u8], _flags: usize) -> Result<usize> {
        let context = {
            let contexts = context::contexts();
            let context = contexts.current().ok_or(Error::new(ESRCH))?;
            Arc::downgrade(&context)
        };

        let inner = {
            let mut schemes = scheme::schemes_mut();
            if schemes.get_name(path).is_some() {
                return Err(Error::new(EEXIST));
            }
            let inner = Arc::new(UserInner::new(context));
            let id = schemes.insert(path.to_vec().into_boxed_slice(), Arc::new(Box::new(UserScheme::new(Arc::downgrade(&inner))))).expect("failed to insert user scheme");
            inner.scheme_id.store(id, Ordering::SeqCst);
            inner
        };

        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        self.handles.write().insert(id, inner);

        Ok(id)
    }

    fn dup(&self, resource: usize) -> Result<usize> {
        let mut handles = self.handles.write();
        let inner = {
            let inner = handles.get(&resource).ok_or(Error::new(EBADF))?;
            inner.clone()
        };

        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        handles.insert(id, inner);

        Ok(id)
    }

    fn read(&self, resource: usize, buf: &mut [u8]) -> Result<usize> {
        let inner = {
            let handles = self.handles.read();
            let inner = handles.get(&resource).ok_or(Error::new(EBADF))?;
            inner.clone()
        };

        inner.read(buf)
    }

    fn write(&self, resource: usize, buf: &[u8]) -> Result<usize> {
        let inner = {
            let handles = self.handles.read();
            let inner = handles.get(&resource).ok_or(Error::new(EBADF))?;
            inner.clone()
        };

        inner.write(buf)
    }

    fn fsync(&self, _resource: usize) -> Result<usize> {
        Ok(0)
    }

    fn close(&self, resource: usize) -> Result<usize> {
        self.handles.write().remove(&resource).ok_or(Error::new(EBADF)).and(Ok(0))
    }
}
