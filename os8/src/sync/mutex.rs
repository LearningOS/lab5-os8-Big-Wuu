use super::UPSafeCell;
use crate::task::TaskControlBlock;
use crate::task::{add_task, current_task, current_process};
use crate::task::{block_current_and_run_next, suspend_current_and_run_next};
use alloc::{collections::VecDeque, sync::Arc};

pub trait Mutex: Sync + Send {
    fn lock(&self);
    fn unlock(&self);
}

pub struct MutexSpin {
    locked: UPSafeCell<bool>,
}

impl MutexSpin {
    pub fn new() -> Self {
        Self {
            locked: unsafe { UPSafeCell::new(false) },
        }
    }
}

impl Mutex for MutexSpin {
    fn lock(&self) {
        loop {
            let mut locked = self.locked.exclusive_access();
            if *locked {
                drop(locked);
                suspend_current_and_run_next();
                continue;
            } else {
                let process = current_process();
                let mut process_inner = process.inner_exclusive_access();
                let mutex_id = process_inner.id;
                let tid = current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid;

                process_inner.mutex_available[mutex_id] -= 1;
                process_inner.mutex_need[tid][mutex_id] -= 1;
                process_inner.mutex_allocation[tid][mutex_id] += 1;
                *locked = true;
                return;
            }
        }
    }

    fn unlock(&self) {
        let mut locked = self.locked.exclusive_access();
        *locked = false;
    }
}

pub struct MutexBlocking {
    inner: UPSafeCell<MutexBlockingInner>,
}

pub struct MutexBlockingInner {
    locked: bool,
    wait_queue: VecDeque<Arc<TaskControlBlock>>,
}

impl MutexBlocking {
    pub fn new() -> Self {
        Self {
            inner: unsafe {
                UPSafeCell::new(MutexBlockingInner {
                    locked: false,
                    wait_queue: VecDeque::new(),
                })
            },
        }
    }
}

impl Mutex for MutexBlocking {
    fn lock(&self) {
        let mut mutex_inner = self.inner.exclusive_access();
        if mutex_inner.locked {
            mutex_inner.wait_queue.push_back(current_task().unwrap());
            drop(mutex_inner);
            block_current_and_run_next();
        } else {
            let process = current_process();
            let mut process_inner = process.inner_exclusive_access();
            let mutex_id = process_inner.id;
            let tid = current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid;

            process_inner.mutex_available[mutex_id] -= 1;
            process_inner.mutex_need[tid][mutex_id] -= 1;
            process_inner.mutex_allocation[tid][mutex_id] += 1;
            mutex_inner.locked = true;
        }
    }

    fn unlock(&self) {
        let mut mutex_inner = self.inner.exclusive_access();
        assert!(mutex_inner.locked);
        if let Some(waking_task) = mutex_inner.wait_queue.pop_front() {
            add_task(waking_task);
        } else {
            mutex_inner.locked = false;
        }
    }
}
