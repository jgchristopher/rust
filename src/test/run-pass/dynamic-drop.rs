// Copyright 2016 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![feature(rustc_attrs)]

use std::cell::{Cell, RefCell};
use std::panic;
use std::usize;

struct InjectedFailure;

struct Allocator {
    data: RefCell<Vec<bool>>,
    failing_op: usize,
    cur_ops: Cell<usize>,
}

impl panic::UnwindSafe for Allocator {}
impl panic::RefUnwindSafe for Allocator {}

impl Drop for Allocator {
    fn drop(&mut self) {
        let data = self.data.borrow();
        if data.iter().any(|d| *d) {
            panic!("missing free: {:?}", data);
        }
    }
}

impl Allocator {
    fn new(failing_op: usize) -> Self {
        Allocator {
            failing_op: failing_op,
            cur_ops: Cell::new(0),
            data: RefCell::new(vec![])
        }
    }
    fn alloc(&self) -> Ptr {
        self.cur_ops.set(self.cur_ops.get() + 1);

        if self.cur_ops.get() == self.failing_op {
            panic!(InjectedFailure);
        }

        let mut data = self.data.borrow_mut();
        let addr = data.len();
        data.push(true);
        Ptr(addr, self)
    }
}

struct Ptr<'a>(usize, &'a Allocator);
impl<'a> Drop for Ptr<'a> {
    fn drop(&mut self) {
        match self.1.data.borrow_mut()[self.0] {
            false => {
                panic!("double free at index {:?}", self.0)
            }
            ref mut d => *d = false
        }

        self.1.cur_ops.set(self.1.cur_ops.get()+1);

        if self.1.cur_ops.get() == self.1.failing_op {
            panic!(InjectedFailure);
        }
    }
}

#[rustc_mir]
fn dynamic_init(a: &Allocator, c: bool) {
    let _x;
    if c {
        _x = Some(a.alloc());
    }
}

#[rustc_mir]
fn dynamic_drop(a: &Allocator, c: bool) {
    let x = a.alloc();
    if c {
        Some(x)
    } else {
        None
    };
}

#[rustc_mir]
fn assignment2(a: &Allocator, c0: bool, c1: bool) {
    let mut _v = a.alloc();
    let mut _w = a.alloc();
    if c0 {
        drop(_v);
    }
    _v = _w;
    if c1 {
        _w = a.alloc();
    }
}

#[rustc_mir]
fn assignment1(a: &Allocator, c0: bool) {
    let mut _v = a.alloc();
    let mut _w = a.alloc();
    if c0 {
        drop(_v);
    }
    _v = _w;
}

fn run_test<F>(mut f: F)
    where F: FnMut(&Allocator)
{
    let first_alloc = Allocator::new(usize::MAX);
    f(&first_alloc);

    for failing_op in 1..first_alloc.cur_ops.get()+1 {
        let alloc = Allocator::new(failing_op);
        let alloc = &alloc;
        let f = panic::AssertUnwindSafe(&mut f);
        let result = panic::catch_unwind(move || {
            f.0(alloc);
        });
        match result {
            Ok(..) => panic!("test executed {} ops but now {}",
                             first_alloc.cur_ops.get(), alloc.cur_ops.get()),
            Err(e) => {
                if e.downcast_ref::<InjectedFailure>().is_none() {
                    panic::resume_unwind(e);
                }
            }
        }
    }
}

fn main() {
    run_test(|a| dynamic_init(a, false));
    run_test(|a| dynamic_init(a, true));
    run_test(|a| dynamic_drop(a, false));
    run_test(|a| dynamic_drop(a, true));

    run_test(|a| assignment2(a, false, false));
    run_test(|a| assignment2(a, false, true));
    run_test(|a| assignment2(a, true, false));
    run_test(|a| assignment2(a, true, true));

    run_test(|a| assignment1(a, false));
    run_test(|a| assignment1(a, true));
}
