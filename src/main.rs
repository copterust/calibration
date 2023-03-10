#![no_std]
#![no_main]
#![feature(core_intrinsics)]

use core::fmt::Write;
use core::intrinsics;
use core::panic::PanicInfo;

use asm_delay::AsmDelay;
use cortex_m_rt::{entry, exception, ExceptionFrame};
use hal::pac::interrupt;
use hal::prelude::*;
use hal::serial;
use hal::time::Bps;

use mpu9250::Mpu9250;

static mut L: Option<hal::serial::Tx<hal::pac::USART2>> = None;
static mut RX: Option<hal::serial::Rx<hal::pac::USART2>> = None;
static mut QUIET: bool = true;
static mut NOW_MS: u32 = 0;
const TURN_QUIET: u8 = 'q' as u8;

#[entry]
fn main() -> ! {
    let device = hal::pac::Peripherals::take().unwrap();
    let core = cortex_m::Peripherals::take().unwrap();
    let mut rcc = device.RCC.constrain();
    let mut flash = device.FLASH.constrain();
    let clocks = rcc
        .cfgr
        .sysclk(64.mhz())
        .pclk1(32.mhz())
        .pclk2(32.mhz())
        .freeze(&mut flash.acr);
    let gpioa = device.GPIOA.split(&mut rcc.ahb);
    let gpiob = device.GPIOB.split(&mut rcc.ahb);
    let mut serial =
        device
            .USART2
            .serial((gpioa.pa2, gpioa.pa15), Bps(460800), clocks);
    let ser_int = serial.get_interrupt();
    serial.listen(serial::Event::Rxne);
    let (mut tx, rx) = serial.split();
    writeln!(tx, "tx ok").unwrap();
    unsafe {
        L = Some(tx);
        RX = Some(rx);
    };
    let l = unsafe { extract(&mut L) };
    writeln!(l, "logger ok").unwrap();
    // SPI1
    let ncs = gpiob.pb0.output().push_pull();
    let spi = device.SPI1.spi(
        // scl_sck, ad0_sd0_miso, sda_sdi_mosi,
        (gpioa.pa5, gpiob.pb4, gpiob.pb5),
        mpu9250::MODE,
        1.mhz(),
        clocks,
    );
    writeln!(l, "spi ok").unwrap();
    let mut delay = AsmDelay::new(clocks.sysclk());
    writeln!(l, "delay ok").unwrap();
    let mut mpu = match Mpu9250::marg_default(spi, ncs, &mut delay) {
        Ok(m) => m,
        Err(e) => {
            writeln!(l, "Mpu init error: {:?}", e).unwrap();
            panic!("mpu err");
        }
    };
    writeln!(l, "mpu ok").unwrap();
    let mag_health = mpu.magnetometer_healthy();
    writeln!(l, "mag health: {:?}", mag_health);
    loop {};
}

unsafe fn extract<T>(opt: &'static mut Option<T>) -> &'static mut T {
    match opt {
        Some(ref mut x) => &mut *x,
        None => panic!("extract"),
    }
}

fn now_ms() -> u32 {
    unsafe { core::ptr::read_volatile(&NOW_MS as *const u32) }
}

#[exception]
unsafe fn SysTick() {
    NOW_MS = NOW_MS.wrapping_add(1);
}

#[exception]
unsafe fn HardFault(ef: &ExceptionFrame) -> ! {
    let l = extract(&mut L);
    write!(l, "hard fault at {:?}", ef).unwrap();
    panic!("HardFault at {:#?}", ef);
}

#[exception]
unsafe fn DefaultHandler(irqn: i16) {
    let l = extract(&mut L);
    write!(l, "Interrupt: {}", irqn).unwrap();
}

#[panic_handler]
fn panic(panic_info: &PanicInfo) -> ! {
    match unsafe { &mut L } {
        Some(ref mut l) => {
            let payload = panic_info.payload().downcast_ref::<&str>();
            match (panic_info.location(), payload) {
                (Some(location), Some(msg)) => {
                    write!(
                        l,
                        "\r\npanic in file '{}' at line {}: {:?}\r\n",
                        location.file(),
                        location.line(),
                        msg
                    )
                    .unwrap();
                }
                (Some(location), None) => {
                    write!(
                        l,
                        "panic in file '{}' at line {}",
                        location.file(),
                        location.line()
                    )
                    .unwrap();
                }
                (None, Some(msg)) => {
                    write!(l, "panic: {:?}", msg).unwrap();
                }
                (None, None) => {
                    write!(l, "panic occured, no info available").unwrap();
                }
            }
        }
        None => {}
    }
    intrinsics::abort()
}
