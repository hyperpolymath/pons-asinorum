// SPDX-License-Identifier: MPL-2.0

pub mod constant_condition;
pub mod div_by_literal_zero;
pub mod empty_effect_loop;
pub mod self_assignment;
pub mod string_concat_in_loop;
pub mod swallowed_error;
pub mod unreachable_after_jump;
pub mod while_true_no_break;
