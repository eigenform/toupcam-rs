

use pcap::*;
use pretty_hex::*;
use std::convert::TryInto;


#[derive(Debug, Eq, PartialEq)]
enum UrbTransferType {
    Intr = 0x01,
    Ctrl = 0x02,
    Bulk = 0x03,
}
impl From<u8> for UrbTransferType {
    fn from(x: u8) -> Self {
        match x {
            0x01 => Self::Intr,
            0x02 => Self::Ctrl,
            0x03 => Self::Bulk,
            _ => unreachable!("undef transfer type {:02x}", x),
        }
    }
}

#[derive(Debug)]
struct ControlPacket {
    ep: u8,
    rt: u8,
    req: u8,
    val: u16,
    idx: u16,
    len: u16,
}
impl From<&[u8; 64]> for ControlPacket {
    fn from(x: &[u8; 64]) -> Self {
        Self {
            ep: x[0x0a],
            rt: x[0x28],
            req: x[0x29],
            val: u16::from_le_bytes(x[0x2a..=0x2b].try_into().unwrap()),
            idx: u16::from_le_bytes(x[0x2c..=0x2d].try_into().unwrap()),
            len: u16::from_le_bytes(x[0x2e..=0x2f].try_into().unwrap()),
        }
    }
}

fn main() -> Result<(), &'static str> {
    // NOTE: Might be a different bus on *your* machine
    let mut cap = Capture::from_device("usbmon8").expect("usbmon not loaded")
        .immediate_mode(true)
        .open()
        .unwrap();

    let mut key: Option<u16> = None;
    while let Ok(p) = cap.next() {
        // Only interested in control packets for now
        let tt  = UrbTransferType::from(p.data[0x09]);
        if tt != UrbTransferType::Ctrl { continue; }

        // Skip over URB_COMPLETE packets
        if p.data[0x08] == 0x43 { continue; }

        let mut p = ControlPacket::from(&p.data[0x00..0x040].try_into().unwrap());
        match p.req {
            0x17 => { 
                key = None; 
            }
            0x16 => {
                let val = p.val.rotate_right(4);
                key = Some(val);
                println!("[!] Set key to {:04x}", val);
            },
            0x0a | 0x0b => {
                if let Some(kv) = key {
                    p.val = p.val ^ kv;
                    p.idx = p.idx ^ kv;
                }
            }
            _ => {},
        }

        println!("{:04x?}", p);
    }

    Ok(())

}
