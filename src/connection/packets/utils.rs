use std::f32::consts::PI;

pub fn least_significant(value: u8) -> u8 {
    value & 0b1111
}

pub fn most_significant(value: u8) -> u8 {
    value >> 4
}


pub fn read_float_radians_from_packet(raw_value: u32, num_bits: u32) -> f32 {    
    // Create bitmask for the sign bit (MSB)
    let sign_bitmask = 1 << (num_bits - 1);
    
    // Create bitmask for the magnitude bits (all bits except sign)
    let non_sign_bitmask = sign_bitmask - 1;
    
    // Extract the magnitude (absolute value) by masking out the sign bit
    let discrete_value = raw_value & non_sign_bitmask;
    
    // Maximum possible magnitude value
    let maximum_value = non_sign_bitmask;
    
    // Calculate the fraction of the full circle this represents
    let angle_fraction = discrete_value as f32 / maximum_value as f32;
    
    // Convert to radians (full circle = 2π)
    let final_angle = angle_fraction * 2.0 * PI;
    
    // Check if the original value was negative by testing the sign bit
    if (sign_bitmask & raw_value) != 0 {
        // If negative, return negative angle
        -final_angle
    } else {
        // If positive, return positive angle
        final_angle
    }
}

pub fn limited_string(input: &str, capacity: usize) -> Vec<u8> {
    let mut output = vec![0; capacity];
    let bytes = input.as_bytes();
    let len = bytes.len().min(capacity);
    output[..len].copy_from_slice(&bytes[..len]);
    output
}

pub fn broken_bit(most_significant: u8, least_significant: u8) -> u8 {
    (most_significant & 0x0F) | (least_significant & 0x0F) << 4
}

pub fn read_u16_le(buf: &mut Vec<u8>) -> u16 {
    if buf.len() < 2 {
        panic!("Buffer too short to read u16");
    }
    let bytes = buf.drain(..2).collect::<Vec<u8>>();
    u16::from_le_bytes(bytes.try_into().expect("Slice with incorrect length for u16"))
}

pub fn read_u32_le(buf: &mut Vec<u8>) -> u32 {
    if buf.len() < 4 {
        panic!("Buffer too short to read u32");
    }
    let bytes = buf.drain(..4).collect::<Vec<u8>>();
    u32::from_le_bytes(bytes.try_into().expect("Slice with incorrect length for u32"))
}

pub fn read_u8(buf: &mut Vec<u8>) -> u8 {
    if buf.is_empty() {
        panic!("Buffer too short to read u8");
    }
    buf.drain(..1).as_slice().to_vec()[0]
}