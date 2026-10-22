use emucore::{
    cpu::Width,
    flags::{AUXILIARY, Alu, CARRY, Condition, OVERFLOW, PARITY, SIGN, ZERO},
};

#[test]
fn exhaustively_checks_byte_addition_and_subtraction_against_integer_math() {
    for left in 0..=255_u64 {
        for right in 0..=255_u64 {
            let (sum, add) = Alu::Add.evaluate(left, right, Width::Byte, 0x202);
            let (difference, sub) = Alu::Sub.evaluate(left, right, Width::Byte, 0x202);
            let signed_sum = i16::from(left as u8 as i8) + i16::from(right as u8 as i8);
            let signed_difference = i16::from(left as u8 as i8) - i16::from(right as u8 as i8);
            assert_eq!(sum, (left + right) % 256);
            assert_eq!(
                difference,
                u64::from((left as u8).wrapping_sub(right as u8))
            );
            assert_eq!(add & CARRY != 0, left + right > 255);
            assert_eq!(sub & CARRY != 0, left < right);
            assert_eq!(add & OVERFLOW != 0, !(-128..=127).contains(&signed_sum));
            assert_eq!(
                sub & OVERFLOW != 0,
                !(-128..=127).contains(&signed_difference)
            );
            assert_eq!(add & AUXILIARY != 0, (left % 16) + (right % 16) > 15);
            assert_eq!(sub & AUXILIARY != 0, left % 16 < right % 16);
            for (result, flags) in [(sum, add), (difference, sub)] {
                assert_eq!(flags & ZERO != 0, result == 0);
                assert_eq!(flags & SIGN != 0, result >= 128);
                assert_eq!(
                    flags & PARITY != 0,
                    (result as u8).count_ones().is_multiple_of(2)
                );
                assert_eq!(flags & 0x202, 0x202);
            }
            assert_eq!(
                Condition::Less.test(sub),
                (left as u8 as i8) < (right as u8 as i8)
            );
            assert_eq!(
                Condition::GreaterOrEqual.test(sub),
                (left as u8 as i8) >= (right as u8 as i8)
            );
            assert_eq!(Condition::BelowOrEqual.test(sub), left <= right);
            assert_eq!(Condition::Above.test(sub), left > right);
        }
    }
}

#[test]
fn wider_boundaries_and_increment_carry_preservation() {
    for width in [Width::Word, Width::Dword, Width::Qword] {
        let (result, flags) = Alu::Add.evaluate(width.mask(), 1, width, 0x202);
        assert_eq!(result, 0);
        assert_ne!(flags & CARRY, 0);
        let sign = 1_u64 << (width.bytes() * 8 - 1);
        assert_ne!(Alu::Add.evaluate(sign - 1, 1, width, 0x202).1 & OVERFLOW, 0);
        assert_ne!(Alu::Sub.evaluate(sign, 1, width, 0x202).1 & OVERFLOW, 0);
    }
    for carry in [0, CARRY] {
        for value in 0..=255 {
            let (increment, flags) = Alu::Increment.evaluate(value, 0, Width::Byte, 0x202 | carry);
            let (decrement, other) = Alu::Decrement.evaluate(value, 0, Width::Byte, 0x202 | carry);
            assert_eq!(increment, (value + 1) % 256);
            assert_eq!(decrement, (value + 255) % 256);
            assert_eq!(flags & CARRY, carry);
            assert_eq!(other & CARRY, carry);
        }
    }
}

#[test]
fn logical_operations_clear_carry_overflow_and_have_defined_prototype_af() {
    for operation in [Alu::And, Alu::Or, Alu::Xor, Alu::Test] {
        let (_, flags) = operation.evaluate(
            0x80,
            0xff,
            Width::Byte,
            0x202 | CARRY | OVERFLOW | AUXILIARY,
        );
        assert_eq!(flags & (CARRY | OVERFLOW | AUXILIARY), 0);
    }
    assert!(!Alu::Compare.writes_result());
    assert!(!Alu::Test.writes_result());
}
