use rsvz_abi_macros::pvz_abi_call;

fn main() {
    let app = 1usize;
    let value = 2usize;
    let out: usize;

    unsafe {
        pvz_abi_call! {
            addr: 1,
            this: ebp = app,
        }
    }

    unsafe {
        pvz_abi_call! {
            addr: 1,
            regs: { ebp = value },
        }
    }

    unsafe {
        pvz_abi_call! {
            addr: 1,
            ret: ebp => out,
        }
    }

    unsafe {
        pvz_abi_call! {
            addr: 1,
            clobber: [ebp],
        }
    }
}
