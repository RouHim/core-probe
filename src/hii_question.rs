#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HiiQuestion {
    pub name: String,
    pub answer: String,
    pub help: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_default_hii_question_when_creating_then_all_fields_empty() {
        let question = HiiQuestion::default();

        assert_eq!(question.name, "");
        assert_eq!(question.answer, "");
        assert_eq!(question.help, "");
    }

    #[test]
    fn given_populated_hii_question_when_cloning_then_fields_match() {
        let original = HiiQuestion {
            name: "Test Name".to_string(),
            answer: "Test Answer".to_string(),
            help: "Test Help".to_string(),
        };

        let cloned = original.clone();

        assert_eq!(cloned.name, original.name);
        assert_eq!(cloned.answer, original.answer);
        assert_eq!(cloned.help, original.help);
    }

    #[test]
    fn given_two_identical_questions_when_comparing_then_equal() {
        let question1 = HiiQuestion {
            name: "Question".to_string(),
            answer: "Answer".to_string(),
            help: "Help Text".to_string(),
        };
        let question2 = HiiQuestion {
            name: "Question".to_string(),
            answer: "Answer".to_string(),
            help: "Help Text".to_string(),
        };

        assert_eq!(question1, question2);
    }
}
