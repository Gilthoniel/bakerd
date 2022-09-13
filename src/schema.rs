table! {
    account_rewards (id) {
        id -> Integer,
        account_id -> Integer,
        block_hash -> Text,
        epoch_ms -> BigInt,
        kind -> Text,
        amount -> Text,
    }
}

table! {
    accounts (id) {
        id -> Integer,
        address -> Text,
        lottery_power -> Double,
        balance -> Text,
        stake -> Text,
        pending_update -> Bool,
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
    hist_prices (pair_id) {
        pair_id -> Integer,
        bid -> Double,
        ask -> Double,
        timestamp_ms -> BigInt,
    }
}

table! {
    pairs (id) {
        id -> Integer,
        base -> Text,
        quote -> Text,
    }
}

table! {
    prices (pair_id) {
        pair_id -> Integer,
        bid -> Double,
        ask -> Double,
        daily_change_relative -> Double,
        high -> Double,
        low -> Double,
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

table! {
    user_sessions (id) {
        id -> Text,
        user_id -> Integer,
        expiration_ms -> BigInt,
        last_use_ms -> BigInt,
    }
}

table! {
    users (id) {
        id -> Integer,
        username -> Text,
        password -> Text,
    }
}

joinable!(account_rewards -> accounts (account_id));
joinable!(hist_prices -> pairs (pair_id));
joinable!(prices -> pairs (pair_id));
joinable!(user_sessions -> users (user_id));

allow_tables_to_appear_in_same_query!(
  account_rewards,
  accounts,
  blocks,
  hist_prices,
  pairs,
  prices,
  statuses,
  user_sessions,
  users,
);
