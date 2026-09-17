# `rsvz-abi-macros`

Backend/support-layer procedural macros for declaring PvZ ABI wrappers and
direct ABI call sites.

## Usage

```toml
[dependencies]
rsvz-abi-macros = { path = "../abi-macros" }
```

```rust
use rsvz_abi_macros::{pvz_abi_call, pvz_abi_fn};

pvz_abi_fn! {
    board_stage_has_pool(board: *mut MainObject) -> u8 {
        addr: 0x41c0d0,
        this: eax = board,
        clobber: [eax],
        ret: al,
    }
}
```

Low-level one-off call sites can still use `pvz_abi_call!` directly:

```rust
let ok: u8;
unsafe {
    pvz_abi_call! {
        addr: 0x41c0d0,
        this: eax = board,
        ret: al => ok,
    }
}
```

## Optional EXE verification

Set `RSVZ_PVZ1051_EXE` to the original 1.0.0.1051 executable and enable the
`verify-exe` feature to check declarations against reachable x86 code. Proven
contradictions are compile errors; properties that static analysis cannot prove
are emitted as one warning per macro invocation.

The declared address remains a manually reviewed function-entry precondition.
Verification rejects addresses outside decodable `.text`, but it does not infer
function boundaries or prove an address from xrefs.
