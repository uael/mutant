from typing import List, Tuple

def bytes_to_int(byte_list: List[int]) -> int:
    """Convert a list of bytes (big-endian) to an integer."""
    value = 0
    for byte in byte_list:
        value = (value << 8) | byte
    return value

def int_to_bytes(value: int, length: int) -> List[int]:
    """Convert an integer to a list of bytes (big-endian) with the specified length."""
    return [(value >> (8 * (length - 1 - i))) & 0xFF for i in range(length)]

def find_sub_ranges(start_bytes: List[int], end_bytes: List[int]) -> List[Tuple[List[str], List[str]]]:
    """
    Decompose the inclusive range from start_bytes to end_bytes into sub-ranges.
    Each sub-range is represented as a tuple of two lists:
    - Fixed bytes (or ranges)
    - Variable bytes (denoted by '00..FF' or specific ranges)
    """
    start = bytes_to_int(start_bytes)
    end = bytes_to_int(end_bytes)
    length = len(start_bytes)
    sub_ranges = []

    current = start

    while current <= end:
        max_block = end - current + 1

        # Determine the number of trailing zero bits to find alignment
        trailing_zeros = 0
        while (current & (0xFF << (8 * trailing_zeros))) == 0 and trailing_zeros < length:
            trailing_zeros += 1

        # Calculate the size of the block
        block_size = 1
        for i in range(trailing_zeros, length):
            possible_size = 256 ** (length - i -1)
            if possible_size > max_block:
                break
            block_size = possible_size
            trailing_zeros +=1

        # Determine the number of fixed bytes
        fixed_bytes = []
        variable_bytes = []
        for i in range(length):
            byte_shift = 8 * (length - 1 - i)
            byte_value = (current >> byte_shift) & 0xFF
            end_byte_value = (current + block_size -1) >> byte_shift & 0xFF

            if block_size >= 256 ** (length - i -1):
                fixed_bytes.append(f"0x{byte_value:02X}")
            else:
                fixed_bytes.append(f"0x{byte_value:02X}")
                # Variable byte range
                variable_start = (current >> (8 * (length - i -1))) & 0xFF
                variable_end = (current + block_size -1) >> (8 * (length - i -1)) & 0xFF
                variable_bytes.append(f"0x{variable_start:02X}..0x{variable_end:02X}")
                break  # Lower bytes are fully variable

        # Fill the remaining bytes with variable ranges if any
        for j in range(len(fixed_bytes), length):
            variable_bytes.append("0x00..0xFF")

        sub_ranges.append((
            fixed_bytes.copy(),
            variable_bytes.copy()
        ))

        current += block_size

    # Simplify the representation
    simplified_ranges = []
    for fixed, variable in sub_ranges:
        range_repr = []
        for i in range(length):
            if i < len(fixed):
                range_repr.append(fixed[i])
            if i < len(variable):
                range_repr.append(variable[i])
        simplified_ranges.append(range_repr)

    return simplified_ranges

def format_sub_ranges(sub_ranges: List[List[str]]) -> str:
    """Format the list of sub-ranges into a readable string."""
    lines = []
    for sub_range in sub_ranges:
        formatted = "[" + ", ".join(sub_range) + "]"
        lines.append(formatted)
    return " | \n  ".join(lines)

# Example Usage
if __name__ == "__main__":
    start = [0x11, 0x22, 0x33, 0x44]
    end = [0x11, 0x22, 0xAB, 0xCD]
    sub_ranges = find_sub_ranges(start, end)
    formatted_output = format_sub_ranges(sub_ranges)
    print("Decomposed Sub-Ranges:")
    print(formatted_output)