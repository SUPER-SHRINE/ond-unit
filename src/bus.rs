use crate::config::{Access, Case, Config, Operation};
use kagura::bus::{Bus, BusFault};

pub struct TestBus<'a> {
    pub ram: Vec<u8>,
    pub devices: Vec<Mock<'a>>,
    pub mismatch: Option<String>,
}
pub struct Mock<'a> {
    pub name: &'a str,
    pub base: u64,
    pub end: u64,
    responses: &'a [crate::config::Response],
    pub consumed: usize,
    pub expected: Option<&'a [Access]>,
    pub trace: Vec<Access>,
}
impl<'a> TestBus<'a> {
    pub fn new(config: &'a Config, case: &'a Case) -> Result<Self, String> {
        let size =
            usize::try_from(config.vm.ram_size).map_err(|_| "RAM容量はhostで表現できません")?;
        let mut ram = Vec::new();
        ram.try_reserve_exact(size)
            .map_err(|e| format!("RAM確保: {e}"))?;
        ram.resize(size, 0);
        let devices = config
            .mmio
            .iter()
            .map(|dev| {
                let case_device = case.mmio.iter().find(|d| d.device == dev.name);
                Mock {
                    name: &dev.name,
                    base: u64::from(dev.base),
                    end: u64::from(dev.base) + dev.size,
                    responses: case_device.map_or(&[], |d| d.read_responses.as_slice()),
                    consumed: 0,
                    expected: case_device.and_then(|d| d.expected_accesses.as_deref()),
                    trace: Vec::new(),
                }
            })
            .collect();
        Ok(Self {
            ram,
            devices,
            mismatch: None,
        })
    }
    fn read(&mut self, address: u32, width: u8) -> Result<u32, BusFault> {
        let start = u64::from(address);
        let end = start + u64::from(width);
        if end <= self.ram.len() as u64 {
            return Ok(self.ram[address as usize..end as usize]
                .iter()
                .enumerate()
                .fold(0, |v, (i, b)| v | (u32::from(*b) << (8 * i))));
        }
        let dev = self
            .devices
            .iter_mut()
            .find(|d| start >= d.base && end <= d.end)
            .ok_or(BusFault)?;
        let position = dev.consumed;
        let reason = match dev.responses.get(position) {
            Some(r) if r.width == width => None,
            Some(r) => Some(format!(
                "応答幅不一致: device={} address=0x{address:08x} width={width} response_position={position} response_width={}",
                dev.name, r.width
            )),
            None => Some(format!(
                "応答枯渇: device={} address=0x{address:08x} width={width} response_position={position}",
                dev.name
            )),
        };
        if let Some(reason) = reason {
            self.mismatch = Some(reason);
            return Err(BusFault);
        }
        let value = dev.responses[position].value;
        dev.consumed += 1;
        dev.trace.push(Access {
            operation: Operation::Read,
            address,
            width,
            value,
        });
        Ok(value)
    }
    fn write(&mut self, address: u32, width: u8, value: u32) -> Result<(), BusFault> {
        let start = u64::from(address);
        let end = start + u64::from(width);
        if end <= self.ram.len() as u64 {
            self.ram[address as usize..end as usize]
                .copy_from_slice(&value.to_le_bytes()[..width as usize]);
            return Ok(());
        }
        let dev = self
            .devices
            .iter_mut()
            .find(|d| start >= d.base && end <= d.end)
            .ok_or(BusFault)?;
        dev.trace.push(Access {
            operation: Operation::Write,
            address,
            width,
            value,
        });
        Ok(())
    }
    pub fn expectations(&self, case: &Case) -> Vec<String> {
        let mut failures = Vec::new();
        for r in &case.ram_expected {
            let actual = &self.ram[r.address as usize..r.bounds().1 as usize];
            if actual != r.bytes {
                failures.push(format!(
                    "RAM期待値不一致: address=0x{:08x} expected={:02x?} actual={actual:02x?}",
                    r.address, r.bytes
                ));
            }
        }
        for d in &self.devices {
            if let Some(expected) = d.expected
                && expected != d.trace
            {
                failures.push(format!(
                    "MMIO期待列不一致: device={} expected={expected:?} actual={:?}",
                    d.name, d.trace
                ));
            }
        }
        failures
    }
}
impl Bus for TestBus<'_> {
    fn read8(&mut self, a: u32) -> Result<u8, BusFault> {
        self.read(a, 1).map(|v| v as u8)
    }
    fn read16(&mut self, a: u32) -> Result<u16, BusFault> {
        self.read(a, 2).map(|v| v as u16)
    }
    fn read32(&mut self, a: u32) -> Result<u32, BusFault> {
        self.read(a, 4)
    }
    fn write8(&mut self, a: u32, v: u8) -> Result<(), BusFault> {
        self.write(a, 1, u32::from(v))
    }
    fn write16(&mut self, a: u32, v: u16) -> Result<(), BusFault> {
        self.write(a, 2, u32::from(v))
    }
    fn write32(&mut self, a: u32, v: u32) -> Result<(), BusFault> {
        self.write(a, 4, v)
    }
}
