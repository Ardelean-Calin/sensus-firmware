#![no_std]

use core::future::Future;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering;
use core::task::Poll;
use embassy_sync::waitqueue::AtomicWaker;

pub struct AsyncGuardTrue<'a> {
    key: &'a AtomicBool,
    waker: &'a AtomicWaker,
}

pub struct AsyncGuardFalse<'a> {
    key: &'a AtomicBool,
    waker: &'a AtomicWaker,
}

impl<'a> Future for AsyncGuardTrue<'a> {
    type Output = ();

    fn poll(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> Poll<Self::Output> {
        self.waker.register(cx.waker());

        if self.key.load(Ordering::Relaxed) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

impl<'a> Future for AsyncGuardFalse<'a> {
    type Output = ();

    fn poll(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> Poll<Self::Output> {
        self.waker.register(cx.waker());

        if !self.key.load(Ordering::Relaxed) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

pub struct AsyncGuard {
    key: AtomicBool,
    waker: AtomicWaker,
}

impl AsyncGuard {
    pub const fn new() -> Self {
        AsyncGuard {
            key: AtomicBool::new(false),
            waker: AtomicWaker::new(),
        }
    }
    pub fn is_true<'a>(&'static self) -> AsyncGuardTrue<'a> {
        AsyncGuardTrue {
            key: &self.key,
            waker: &self.waker,
        }
    }

    pub fn is_false<'a>(&'static self) -> AsyncGuardFalse<'a> {
        AsyncGuardFalse {
            key: &self.key,
            waker: &self.waker,
        }
    }

    pub fn ready(&self, val: bool) {
        self.key.store(val, Ordering::Relaxed);
        self.waker.wake();
    }
}
