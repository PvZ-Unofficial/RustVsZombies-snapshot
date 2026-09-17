use rsvz_abi_macros::pvz_abi_call;

fn main() {
    let board = core::ptr::null_mut::<u8>();
    let flag = 1u8;
    unsafe {
        pvz_abi_call! {
            addr: 1,
            this: eax = board,
            regs: { al = flag },
        }
    }
}
