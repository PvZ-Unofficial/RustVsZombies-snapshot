use rsvz_abi_macros::pvz_abi_call;

fn main() {
    let this_arg = core::ptr::null_mut::<u8>();
    let a = 1usize;
    let b = 2usize;
    let c = 3usize;
    let d = 4usize;
    let e = 5usize;
    unsafe {
        pvz_abi_call! {
            addr: 1,
            this: eax = this_arg,
            regs: {
                ebx = a,
                ecx = b,
                edx = c,
                esi = d,
                edi = e,
            },
        }
    }
}
