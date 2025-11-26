use indexmap::IndexMap;
use thiserror::Error as ThisError;

use crate::input;

#[derive(Debug, ThisError)]
pub enum ProcessingSequenceError {
    #[error("Duplicate Sequence Name, Sequence {0}")]
    DuplicateSequenceName(usize),
    #[error("Model Has Too Many Sequences")]
    TooManySequences,
}

pub fn process_sequences(input_data: &input::SourceInput, remapped_animations: &[usize]) -> Result<IndexMap<String, super::Sequence>, ProcessingSequenceError> {
    let mut processed_sequences = IndexMap::with_capacity(input_data.sequences.len());

    for (input_sequence_index, input_sequence) in input_data.sequences.iter().enumerate() {
        let processed_sequence_name = input_sequence.name.clone();
        if processed_sequences.contains_key(&processed_sequence_name) {
            return Err(ProcessingSequenceError::DuplicateSequenceName(input_sequence_index + 1));
        }

        let mut processed_sequence = super::Sequence {
            animations: vec![vec![0; input_sequence.animations[0].len()]; input_sequence.animations.len()],
        };

        for (row_index, row_value) in input_sequence.animations.iter().enumerate() {
            for (column_index, column_value) in row_value.iter().enumerate() {
                let mapped_animation_index = remapped_animations[*column_value];

                processed_sequence.animations[row_index][column_index] = mapped_animation_index as i16;
            }
        }

        processed_sequences.insert(processed_sequence_name, processed_sequence);
    }

    if processed_sequences.len() > i32::MAX as usize {
        return Err(ProcessingSequenceError::TooManySequences);
    }

    Ok(processed_sequences)
}
