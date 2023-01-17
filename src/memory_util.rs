pub fn get_alignment_vector(number: usize, alignment: usize) -> Vec<u8> {
    vec![0_u8; (alignment - number % alignment) % alignment]
}