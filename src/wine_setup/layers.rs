//! The pinned DXMT package the Metal layer is unpacked from. Where that layer ends up on the disk,
//! the Mac libraries and the environment Wine runs in are in `backend::engine_layout`.

use super::*;

/// That project's build of DXMT (LGPL-2.1 or later). Unlike DXMT 0.80 it implements
/// `SwapDeviceContextState`, which Direct2D calls, and lets a browser's GPU process draw into the
/// window of another process (Wine bug 60263).
pub(super) const METAL_LAYER: Pinned = Pinned {
    name: "dxmt-highball-20261017T231707Z-331c552.tar.gz",
    url: "https://github.com/gauthierpiarrette/highball-engine/releases/download/dxmt-highball-20261017T231707Z-331c552/dxmt-highball-20261017T231707Z-331c552.tar.gz",
    sha256: "89e731b81cc1ef741accb2e707159f121ed822a083f14f16da445cd8dbbf4703",
    size: 18_928_717,
};
