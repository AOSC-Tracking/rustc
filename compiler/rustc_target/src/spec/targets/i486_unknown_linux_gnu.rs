use crate::spec::{Target, TargetOptions, LinkerFlavor, Cc, Lld};

pub(crate) fn target() -> Target {
    let mut base = super::i586_unknown_linux_gnu::target();
    base.cpu = "i486".into();
    base.llvm_target = "i486-unknown-linux-gnu".into();
    // FIXME: technically we should set max_atomic_width to 32 because
    // i486 lacks cmpxchg8b but the vendored portable-atomic crate does
    // not handle missing Atomic{I,U}64 on x86 (i.e. it basically
    // believes x86 means i586 or newer).  So use libatomic to simulate
    // 64-bit atomics.  This may be correct or not, and anyway the
    // technically sound fix is adding the i486 support to portable-
    // atomic and then change max_atomic_width instead of this.
    base.options.late_link_args = TargetOptions::link_args(LinkerFlavor::Gnu(Cc::Yes, Lld::No), &["-latomic"]);
    base
}
