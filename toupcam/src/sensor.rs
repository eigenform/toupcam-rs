//! [Private] functions for configuring the CMOS sensor (here be dragons).
//!
//! # Notes
//! Mostly replicated from USB packet captures: the current initialization 
//! sequence is not well understood, and may not be generalizable to different 
//! initial states of the camera. 
//!
//! # Safety
//! The probability of damaging the sensor here is non-zero!
//! I have no idea how safe this is, and no concrete information about how
//! the sensor configuration *actually* works.
//!
//! Additionally, it also seems like particular sequences of commands are
//! sensitive to timing; the tolerances are unclear.
//!

use crate::{ Error, Camera };
use std::time::Duration;

impl Camera {

    /// Apply an initial configuration to the CMOS sensor.
    ///
    /// This corresponds [AFAIK] to the following initial setup:
    ///
    /// 1. Set size to mode 1
    /// 2. Set TOUPCAM_OPTION_RAW to 1
    /// 3. Set TOUPCAM_OPTION_BITDEPTH to 1
    /// 4. Set auto-exposure enable to false
    /// 5. Exposure time is set to 94000us (94ms)?
    ///
    pub (crate) fn sensor_init(&mut self) -> Result<(), Error> {

        self.sys_write(0x0200, 0x0001)?;
        self.sys_write(0x8000, 0x09b0)?;
        self.set_exposure(0x0637, 0x0e24)?;

        // Write sensor configuration (unclear)
        self.sensor_write(0x1008, 0x4299)?; 
        self.sensor_write(0x100f, 0x7fff)?; 
        self.sensor_write(0x1001, 0x0030)?; 
        self.sensor_write(0x1002, 0x0003)?;
        self.sensor_write(0x1003, 0x07e9)?; 
        self.sensor_write(0x1000, 0x0003)?; 
        self.sensor_write(0x1004, 0x0087)?;  // related to mode 0?
        self.sensor_write(0x1006, 0x1104)?;  // related to mode 0?
        self.sensor_write(0x1009, 0x02c0)?; 
        self.sensor_write(0x1005, 0x0001)?; 
        self.sensor_write(0x1007, 0x7fff)?; 
        self.sensor_write(0x100a, 0x0000)?;
        self.sensor_write(0x100b, 0x0100)?; 
        self.sensor_write(0x100c, 0x0000)?; 
        self.sensor_write(0x100d, 0x2090)?; 
        self.sensor_write(0x100e, 0x0103)?;
        self.sensor_write(0x1010, 0x0000)?; 
        self.sensor_write(0x1011, 0x0000)?; 
        std::thread::sleep(Duration::from_millis(5));
        self.sensor_write(0x1000, 0x0053)?; 
        self.sensor_write(0x1008, 0x0298)?;
        std::thread::sleep(Duration::from_millis(5));

        // -------
        self.sys_write(0x1200, 0x0001)?;
        std::thread::sleep(Duration::from_millis(20)); // should be 20?
        self.sys_write(0x2000, 0x0000)?;
        self.sys_write(0x1200, 0x0002)?;
        std::thread::sleep(Duration::from_millis(20)); // should be 20?

        self.sys_write(0x0200, 0x0001)?; // '0x0001' enables 12-bit depth?
        self.sys_write(0x0a00, 0x0001)?;
        std::thread::sleep(Duration::from_millis(20)); // should be 20?
        self.sys_write(0x0a00, 0x0000)?;
        std::thread::sleep(Duration::from_millis(20)); // should be 20?

        // Write sensor configuration (unclear)
        self.sensor_write(0x1008, 0x4299)?; 
        self.sensor_write(0x100f, 0x7fff)?; 
        self.sensor_write(0x1001, 0x0030)?; 
        self.sensor_write(0x1002, 0x0003)?;
        self.sensor_write(0x1003, 0x07e9)?; 
        self.sensor_write(0x1000, 0x0003)?; 
        self.sensor_write(0x1004, 0x0083)?; // related to mode 1/2?
        self.sensor_write(0x1006, 0x11dc)?; // related to mode 1/2?
        self.sensor_write(0x1009, 0x02c0)?; 
        self.sensor_write(0x1005, 0x0001)?; 
        self.sensor_write(0x1007, 0x7fff)?; 
        self.sensor_write(0x100a, 0x0000)?;
        self.sensor_write(0x100b, 0x0100)?; 
        self.sensor_write(0x100c, 0x0000)?; 
        self.sensor_write(0x100d, 0x2090)?; 
        self.sensor_write(0x100e, 0x0103)?;
        self.sensor_write(0x1010, 0x0000)?; 
        self.sensor_write(0x1011, 0x0000)?; 
        std::thread::sleep(Duration::from_millis(5));
        self.sensor_write(0x1000, 0x0053)?; 
        self.sensor_write(0x1008, 0x0298)?;
        std::thread::sleep(Duration::from_millis(5));

        // -------
        self.sys_write(0x103b, 0x0000)?;

        self.sys_write(0x2000, 0x0001)?; // related to mode 1
        self.sys_write(0x1200, 0x0003)?; // related to mode 1
        std::thread::sleep(Duration::from_millis(10));

        // Perhaps resolution related?
        self.sys_write(0x8000, 0x060c)?; // related to mode 1?

        //  94000us - 0x0cbd
        // 150000us - 0x144e
        self.set_exposure(0x000a, 0x0cbd)?;

        self.sys_write(0x0a00, 0x0001)?;
        //std::thread::sleep(Duration::from_millis(10));

        self.set_exposure(0x000a, 0x0cbd)?;
        self.set_analog_gain(0x610c)?;

        Ok(())
    }

    // Set exposure parameters?
    //
    // It seems like `0x1064` and `0x5000` are the only ones that vary.
    // Not clear how this works yet.
    pub (crate) fn set_exposure(&mut self, val1064: u16, val5000: u16) 
        -> Result<(), Error>
    {
        self.sensor_write(0x1063, 0x0000)?;
        self.sensor_write(0x1064, val1064)?;
        self.sys_write(0x4000, 0x0000)?;
        self.sys_write(0x5000, val5000)?;
        Ok(())
    }

    /// Set the analog gain.
    pub (crate) fn set_analog_gain(&mut self, val1061: u16) 
        -> Result<(), Error> 
    {
        self.sensor_write(0x1061, val1061)
    }


    /// Read from EEPROM?
    pub (crate) fn read_eeprom(&mut self) -> Result<(), Error> {
        let mut eeprom_buf_1: [u8; 0x1000] = [0; 0x1000];
        let mut eeprom_buf_2: [u8; 0x0cbb] = [0; 0x0cbb];
        self.ven_in(0x20, 0x0000, 0x0000, &mut eeprom_buf_1)?;
        self.ven_in(0x20, 0x1000, 0x0000, &mut eeprom_buf_2)?;

        use crypto::sha1::*;
        use crypto::digest::*;
        let mut d = Sha1::new();
        d.input(&eeprom_buf_1);
        d.input(&eeprom_buf_2);
        let hex = d.result_str();
        println!("EEPROM SHA1 digest: {}", hex);
        Ok(())
    }
}


