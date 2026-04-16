#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmdGeneration {
    Zen3,
    Zen4,
    Zen5,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoTier {
    Gold,
    Silver,
    Bronze,
    Neutral,
}

pub fn detect_generation(model_name: &str) -> AmdGeneration {
    if !model_name.contains("Ryzen") {
        return AmdGeneration::Unknown;
    }

    if contains_series(model_name, '5') {
        return AmdGeneration::Zen3;
    }

    if contains_phoenix_apu(model_name) || contains_series(model_name, '7') {
        return AmdGeneration::Zen4;
    }

    if contains_series(model_name, '9') {
        return AmdGeneration::Zen5;
    }

    AmdGeneration::Unknown
}

pub fn classify_co(co_value: i32, generation: AmdGeneration) -> CoTier {
    match generation {
        AmdGeneration::Zen3 => classify_by_thresholds(co_value, -25, -15, -5),
        AmdGeneration::Zen4 => classify_by_thresholds(co_value, -28, -18, -8),
        AmdGeneration::Zen5 => classify_by_thresholds(co_value, -30, -20, -10),
        AmdGeneration::Unknown => CoTier::Neutral,
    }
}

fn classify_by_thresholds(
    co_value: i32,
    gold_max: i32,
    silver_max: i32,
    bronze_max: i32,
) -> CoTier {
    if co_value <= gold_max {
        return CoTier::Gold;
    }

    if co_value <= silver_max {
        return CoTier::Silver;
    }

    if co_value <= bronze_max {
        return CoTier::Bronze;
    }

    CoTier::Neutral
}

fn contains_phoenix_apu(model_name: &str) -> bool {
    model_name.contains("8600G") || model_name.contains("8700G")
}

fn contains_series(model_name: &str, leading_digit: char) -> bool {
    model_name
        .split_whitespace()
        .any(|part| matches_generation_token(part, leading_digit))
}

fn matches_generation_token(token: &str, leading_digit: char) -> bool {
    let mut chars = token.chars();

    let Some(first) = chars.next() else {
        return false;
    };
    if first != leading_digit {
        return false;
    }

    let Some(second) = chars.next() else {
        return false;
    };
    if !second.is_ascii_digit() {
        return false;
    }

    let Some(third) = chars.next() else {
        return false;
    };
    if !third.is_ascii_digit() {
        return false;
    }

    let Some(fourth) = chars.next() else {
        return false;
    };
    if !fourth.is_ascii_digit() {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_5900x_model_name_when_detecting_generation_then_returns_zen3() {
        let result = detect_generation("AMD Ryzen 9 5900X 12-Core Processor");

        assert_eq!(result, AmdGeneration::Zen3);
    }

    #[test]
    fn given_5600x_model_name_when_detecting_generation_then_returns_zen3() {
        let result = detect_generation("AMD Ryzen 5 5600X 6-Core Processor");

        assert_eq!(result, AmdGeneration::Zen3);
    }

    #[test]
    fn given_5700g_model_name_when_detecting_generation_then_returns_zen3() {
        let result = detect_generation("AMD Ryzen 7 5700G");

        assert_eq!(result, AmdGeneration::Zen3);
    }

    #[test]
    fn given_7800x3d_model_name_when_detecting_generation_then_returns_zen4() {
        let result = detect_generation("AMD Ryzen 7 7800X3D 8-Core Processor");

        assert_eq!(result, AmdGeneration::Zen4);
    }

    #[test]
    fn given_7950x_model_name_when_detecting_generation_then_returns_zen4() {
        let result = detect_generation("AMD Ryzen 9 7950X 16-Core Processor");

        assert_eq!(result, AmdGeneration::Zen4);
    }

    #[test]
    fn given_8700g_model_name_when_detecting_generation_then_returns_zen4() {
        let result = detect_generation("AMD Ryzen 7 8700G");

        assert_eq!(result, AmdGeneration::Zen4);
    }

    #[test]
    fn given_8600g_model_name_when_detecting_generation_then_returns_zen4() {
        let result = detect_generation("AMD Ryzen 5 8600G");

        assert_eq!(result, AmdGeneration::Zen4);
    }

    #[test]
    fn given_9950x_model_name_when_detecting_generation_then_returns_zen5() {
        let result = detect_generation("AMD Ryzen 9 9950X 16-Core Processor");

        assert_eq!(result, AmdGeneration::Zen5);
    }

    #[test]
    fn given_9800x3d_model_name_when_detecting_generation_then_returns_zen5() {
        let result = detect_generation("AMD Ryzen 7 9800X3D");

        assert_eq!(result, AmdGeneration::Zen5);
    }

    #[test]
    fn given_non_ryzen_model_name_when_detecting_generation_then_returns_unknown() {
        let result = detect_generation("Intel Core i9-13900K");

        assert_eq!(result, AmdGeneration::Unknown);
    }

    #[test]
    fn given_empty_model_name_when_detecting_generation_then_returns_unknown() {
        let result = detect_generation("");

        assert_eq!(result, AmdGeneration::Unknown);
    }

    #[test]
    fn given_zen3_and_minus_25_when_classifying_then_returns_gold() {
        let result = classify_co(-25, AmdGeneration::Zen3);

        assert_eq!(result, CoTier::Gold);
    }

    #[test]
    fn given_zen3_and_minus_24_when_classifying_then_returns_silver() {
        let result = classify_co(-24, AmdGeneration::Zen3);

        assert_eq!(result, CoTier::Silver);
    }

    #[test]
    fn given_zen3_and_minus_15_when_classifying_then_returns_silver() {
        let result = classify_co(-15, AmdGeneration::Zen3);

        assert_eq!(result, CoTier::Silver);
    }

    #[test]
    fn given_zen3_and_minus_14_when_classifying_then_returns_bronze() {
        let result = classify_co(-14, AmdGeneration::Zen3);

        assert_eq!(result, CoTier::Bronze);
    }

    #[test]
    fn given_zen3_and_minus_5_when_classifying_then_returns_bronze() {
        let result = classify_co(-5, AmdGeneration::Zen3);

        assert_eq!(result, CoTier::Bronze);
    }

    #[test]
    fn given_zen3_and_minus_4_when_classifying_then_returns_neutral() {
        let result = classify_co(-4, AmdGeneration::Zen3);

        assert_eq!(result, CoTier::Neutral);
    }

    #[test]
    fn given_zen4_and_minus_28_when_classifying_then_returns_gold() {
        let result = classify_co(-28, AmdGeneration::Zen4);

        assert_eq!(result, CoTier::Gold);
    }

    #[test]
    fn given_zen4_and_minus_27_when_classifying_then_returns_silver() {
        let result = classify_co(-27, AmdGeneration::Zen4);

        assert_eq!(result, CoTier::Silver);
    }

    #[test]
    fn given_zen5_and_minus_30_when_classifying_then_returns_gold() {
        let result = classify_co(-30, AmdGeneration::Zen5);

        assert_eq!(result, CoTier::Gold);
    }

    #[test]
    fn given_zen5_and_minus_29_when_classifying_then_returns_silver() {
        let result = classify_co(-29, AmdGeneration::Zen5);

        assert_eq!(result, CoTier::Silver);
    }

    #[test]
    fn given_unknown_generation_when_classifying_then_returns_neutral() {
        let result = classify_co(-30, AmdGeneration::Unknown);

        assert_eq!(result, CoTier::Neutral);
    }

    #[test]
    fn given_zen3_and_zero_when_classifying_then_returns_neutral() {
        let result = classify_co(0, AmdGeneration::Zen3);

        assert_eq!(result, CoTier::Neutral);
    }

    #[test]
    fn given_zen3_and_positive_15_when_classifying_then_returns_neutral() {
        let result = classify_co(15, AmdGeneration::Zen3);

        assert_eq!(result, CoTier::Neutral);
    }

    #[test]
    fn given_zen3_and_minus_50_when_classifying_then_returns_gold() {
        let result = classify_co(-50, AmdGeneration::Zen3);

        assert_eq!(result, CoTier::Gold);
    }
}
