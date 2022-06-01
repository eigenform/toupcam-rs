# toupcam-rs

Hacky library for interfacing with AmScope MU1603/Touptek U3CMOS16000KPA 
(TP116000A) USB3.0 CMOS cameras. 

- The VID/PID for this model is `0x0547:0x3016`
- Product sheets say the sensor is a [Panasonic] MN34120?

Don't exactly know what I'm doing with this yet.

There are no safety guarantees here, and it's not clear if you can damage the
device by using this (my guess it probably *yes, very*). Proceed at your own risk.

## Prior Art

See John McMaster's work on the MU800:

- [JohnDMcMaster/uscope-cam-wip](https://github.com/JohnDMcMaster/uscope-cam-wip)
- [drivers/media/usb/gspca/touptek.c](https://github.com/torvalds/linux/blob/master/drivers/media/usb/gspca/touptek.c)

