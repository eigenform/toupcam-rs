//! [Private] functions for different kinds of USB control transactions.

use rusb::{ request_type, Direction, RequestType, Recipient };
use crate::{ Error, Camera };

impl Camera {

    /// Write to the sensor registers. 
    pub (crate) fn sensor_write(&mut self, addr: u16, val: u16)
        -> Result<(), Error>
    {
        let mut buf: [u8; 1] = [ 0 ];
        let rt = request_type(Direction::In, RequestType::Vendor, Recipient::Device);

        // Seems like these write to 0x1100 on success?
        self.handle.read_control(rt, 0x0b, val, addr, &mut buf, self.timeout)?;
        if buf[0] == 0x08 {
            self.handle.read_control(rt, 0x0b, val, 0x1100, &mut buf, self.timeout)?;
        } else {
            println!("sensor write to {:04x} returned {:02x}?", addr, buf[0]);
        }
        Ok(())
    }

    // Write to [some other] device registers.
    pub (crate) fn sys_write(&mut self, addr: u16, val: u16) 
        -> Result<(), Error> 
    {
        let mut buf: [u8; 1] = [ 0 ];
        let rt = request_type(Direction::In, RequestType::Vendor, Recipient::Device);
        match self.handle.read_control(rt, 0x0b, val, addr, &mut buf, self.timeout) {
            Ok(_) => Ok(()),
            Err(e) => Err(Error::from(e)),
        }
    }

    /// Send a vendor command (input).
    pub (crate) fn ven_in(&mut self, req: u8, val: u16, idx: u16, buf: &mut [u8]) 
        -> Result<(), Error> 
    {
        let rt = request_type(Direction::In, RequestType::Vendor, Recipient::Device);
        match self.handle.read_control(rt, req, val, idx, buf, self.timeout) {
            Ok(_) => { Ok(()) },
            Err(e) => Err(Error::from(e)),
        }
    }

    /// Send a vendor command (output).
    pub (crate) fn ven_out(&mut self, req: u8, val: u16, idx: u16, buf: &[u8]) 
        -> Result<(), Error> 
    {
        let rt = request_type(Direction::Out, RequestType::Vendor, Recipient::Device);
        match self.handle.write_control(rt, req, val, idx, buf, self.timeout) {
            Ok(_) => { Ok(()) },
            Err(e) => Err(Error::from(e)),
        }
    }
}

