use solana_transaction_status::option_serializer::OptionSerializer;

pub trait StepOptionHelperTrait<T> {
    fn into_option(self) -> Option<T>;
}

impl<T> StepOptionHelperTrait<T> for OptionSerializer<T> {
    fn into_option(self) -> Option<T> {
        match self {
            OptionSerializer::Some(value) => Some(value),
            OptionSerializer::None => None,
            OptionSerializer::Skip => None,
        }
    }
}
