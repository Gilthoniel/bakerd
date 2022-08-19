table! {
    account_rewards (id) {
        id -> Integer,
        account_id -> Integer,
        block_hash -> Text,
        amount -> Text,
        epoch_ms -> BigInt,
        kind -> Text,
    }
}

table! {
    accounts (id) {
        id -> Integer,
        address -> Text,
        available_amount -> Text,
        staked_amount -> Text,
        lottery_power -> Double,
    }
}

table! {
    blocks (id) {
        id -> Integer,
        height -> BigInt,
        hash -> Text,
        slot_time_ms -> BigInt,
        baker -> BigInt,
    }
}

table! {
    prices (base, quote) {
        base -> Text,
        quote -> Text,
        bid -> Double,
        ask -> Double,
    }
}

table! {
    statuses (id) {
        id -> Integer,
        resources -> Text,
        node -> Nullable<Text>,
        timestamp_ms -> BigInt,
    }
}

joinable!(account_rewards -> accounts (account_id));

allow_tables_to_appear_in_same_query!(account_rewards, accounts, blocks, prices, statuses,);
