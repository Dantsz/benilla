//! Wire read/write helpers shared by the auth and world protocols; little endian unless noted.

use std::io::{self, Read, Write};

/// Read a packed GUID: a mask byte naming the non-zero bytes, then only those bytes, low to high.
pub fn read_packed_guid(r: &mut impl Read) -> io::Result<u64> {
    let mask = read_u8(r)?;
    let mut guid = 0u64;
    for i in 0..8 {
        if mask & (1 << i) != 0 {
            guid |= (u64::from(read_u8(r)?)) << (i * 8);
        }
    }
    Ok(guid)
}

/// Write a packed GUID (see [`read_packed_guid`]).
pub fn write_packed_guid(guid: u64, w: &mut impl Write) -> io::Result<()> {
    let bytes = guid.to_le_bytes();
    let mut mask = 0u8;
    let mut out = [0u8; 9];
    let mut idx = 1;
    for (i, &b) in bytes.iter().enumerate() {
        if b != 0 {
            mask |= 1 << i;
            out[idx] = b;
            idx += 1;
        }
    }
    out[0] = mask;
    w.write_all(&out[..idx])
}

pub fn read_u8(r: &mut impl Read) -> io::Result<u8> {
    let mut b = [0u8; 1];
    r.read_exact(&mut b)?;
    Ok(b[0])
}

pub fn read_u16_le(r: &mut impl Read) -> io::Result<u16> {
    let mut b = [0u8; 2];
    r.read_exact(&mut b)?;
    Ok(u16::from_le_bytes(b))
}

pub fn read_u32_le(r: &mut impl Read) -> io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

pub fn read_u32_be(r: &mut impl Read) -> io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_be_bytes(b))
}

pub fn read_i32_le(r: &mut impl Read) -> io::Result<i32> {
    Ok(read_u32_le(r)? as i32)
}

pub fn read_u64_le(r: &mut impl Read) -> io::Result<u64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}

pub fn read_f32_le(r: &mut impl Read) -> io::Result<f32> {
    Ok(f32::from_bits(read_u32_le(r)?))
}

/// Read a fixed-length byte array.
pub fn read_array<const N: usize>(r: &mut impl Read) -> io::Result<[u8; N]> {
    let mut b = [0u8; N];
    r.read_exact(&mut b)?;
    Ok(b)
}

/// Read a NUL-terminated string (the NUL is consumed). Invalid UTF-8 is lossily replaced.
pub fn read_cstring(r: &mut impl Read) -> io::Result<String> {
    let mut bytes = Vec::new();
    loop {
        let b = read_u8(r)?;
        if b == 0 {
            break;
        }
        bytes.push(b);
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// `CDataStore::GetCString` (`0x4191b0`) at `capacity`: the string up to its NUL, which is
/// consumed. With no NUL in the first `capacity` bytes, or none before the end, it reads `None`,
/// the reference's `""` on a poisoned store (`0x4192a1`-`0x4192bb`), past the bytes it scanned.
pub fn read_cstring_capped(r: &mut &[u8], capacity: usize) -> Option<String> {
    let window = &r[..r.len().min(capacity)];
    match window.iter().position(|&b| b == 0) {
        Some(nul) => {
            let s = String::from_utf8_lossy(&window[..nul]).into_owned();
            *r = &r[nul + 1..];
            Some(s)
        }
        None => {
            *r = &r[window.len()..];
            None
        }
    }
}

/// Bound a wire-derived count before it becomes a `Vec::with_capacity` hint: a failed allocation
/// aborts rather than erroring. `cap` is the protocol's own bound where one exists.
pub fn capacity_hint(count: impl TryInto<u64>, cap: usize) -> usize {
    count
        .try_into()
        .ok()
        .and_then(|n| usize::try_from(n).ok())
        .map_or(cap, |n| n.min(cap))
}

/// A 3-float position/vector in raw WoW coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vector3d {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vector3d {
    pub fn read(r: &mut impl Read) -> io::Result<Self> {
        Ok(Self {
            x: read_f32_le(r)?,
            y: read_f32_le(r)?,
            z: read_f32_le(r)?,
        })
    }

    pub fn write(&self, w: &mut impl Write) -> io::Result<()> {
        w.write_all(&self.x.to_le_bytes())?;
        w.write_all(&self.y.to_le_bytes())?;
        w.write_all(&self.z.to_le_bytes())
    }
}

/// Decode a `SMSG_MONSTER_MOVE` packed point: signed 11-bit x, 11-bit y, 10-bit z in quarter yards
/// (`ByteBuffer::appendPackXYZ`), the offset `destination - waypoint`, not a position.
pub fn packed_to_vector3d(p: i32) -> Vector3d {
    // Sign-extend each field: shift it to the top of the i32, then arithmetic-shift back down.
    Vector3d {
        x: ((p << 21) >> 21) as f32 * 0.25,
        y: ((p << 10) >> 21) as f32 * 0.25,
        z: (p >> 22) as f32 * 0.25,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// vmangos's encoder, `ByteBuffer::appendPackXYZ`.
    fn pack_xyz(x: f32, y: f32, z: f32) -> i32 {
        let mut packed = 0u32;
        packed |= ((x * 4.0).round() as i32 & 0x7FF) as u32;
        packed |= (((y * 4.0).round() as i32 & 0x7FF) as u32) << 11;
        packed |= (((z * 4.0).round() as i32 & 0x3FF) as u32) << 22;
        packed as i32
    }

    #[test]
    fn packed_point_roundtrips_signed_quarter_yards() {
        // x and y span −256..255.75 yd, z −128..127.75 yd; quarter yards are exact in f32.
        for &(x, y, z) in &[
            (0.0f32, 0.0f32, 0.0f32),
            (1.25, -2.5, 3.75),
            (-100.25, 100.0, -50.5),
            (255.75, -256.0, 127.75),
            (-0.25, 0.25, -0.25),
        ] {
            let v = packed_to_vector3d(pack_xyz(x, y, z));
            assert_eq!((v.x, v.y, v.z), (x, y, z));
        }
    }
    #[test]
    fn capacity_hint_is_the_smaller_of_count_and_cap() {
        assert_eq!(capacity_hint(3u8, 64), 3);
        assert_eq!(capacity_hint(0xFFFF_FFFFu32, 64), 64);
        assert_eq!(capacity_hint(0u32, 64), 0);
        assert_eq!(capacity_hint(usize::MAX, 8), 8);
        assert_eq!(capacity_hint(u64::MAX, 1024), 1024);
    }
}
