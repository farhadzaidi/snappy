//! Sending the focused window to the back: a press sends it behind the windows it covers
//! (`send`), and holding puts it back as it's let go (`put_back`). Other apps' windows, such
//! as a test VM's terminal, could be stacked there too, so these run on the third workspace,
//! which holds none.

mod put_back;
mod send;
mod support;

pub(crate) use put_back::{
    caps_lock_send_to_back, put_back_floating, put_back_snappy_key_first, put_back_stack,
};
pub(crate) use send::{
    send_to_back_beside_wide, send_to_back_floating, send_to_back_stack, send_to_back_two_apps,
};
