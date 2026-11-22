//! 1.4: pinned conversations are the per-user favourite flag plus the per-user order (`position`); both are stored on
//! the server per person and announced to that person's other devices.
mod common;
use common::*;

async fn prefs(c: &Client) -> Vec<ConvPref> {
    c.get("/me/prefs").await.unwrap()
}

/// Favourite conversations in the person's own order (like the apps' pinned list).
async fn pinned(c: &Client) -> Vec<String> {
    let mut v: Vec<ConvPref> = prefs(c).await.into_iter().filter(|p| p.favourite).collect();
    v.sort_by_key(|p| p.position.unwrap_or(-1));
    v.into_iter().map(|p| p.conv_id).collect()
}

async fn set_pref(c: &Client, conv: &str, req: ConvPrefRequest) -> ConvPref {
    c.put(&format!("/conversations/{conv}/prefs"), &req).await.unwrap()
}

async fn order(c: &Client, ids: &[&String]) {
    let _: Vec<ConvPref> = c.post("/conversations/order", &ConvOrderRequest { conv_ids: ids.iter().map(|s| s.to_string()).collect() }).await.unwrap();
}

#[tokio::test]
async fn pinned_conversations_keep_the_users_order_and_follow_the_user() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let c1 = s.channel("alpha", &[&alice, &bob]).await;
    let c2 = s.channel("beta", &[&alice, &bob]).await;
    let c3 = s.channel("gamma", &[&alice, &bob]).await;
    let mut ws = alice.ws(alice.cursor().await).await;

    // Nothing pinned.
    assert!(pinned(&alice).await.is_empty());

    // Pin gamma, then beta: the newest pin goes first (favourite flag, then the order list).
    for c in [&c3, &c2] {
        let p = set_pref(&alice, c, ConvPrefRequest { favourite: Some(true), ..Default::default() }).await;
        assert!(p.favourite);
        let mut ids: Vec<String> = pinned(&alice).await.into_iter().filter(|i| i != c).collect();
        ids.insert(0, c.clone());
        let refs: Vec<&String> = ids.iter().collect();
        order(&alice, &refs).await;
    }
    assert_eq!(pinned(&alice).await, vec![c2.clone(), c3.clone()]);
    ws.wait_event("pin event", |e| matches!(e, EventData::PrefUpdate { pref } if pref.conv_id == c3 && pref.favourite)).await;
    // Another device sees the same through sync.
    let phone = s.login("alice").await;
    assert_eq!(pinned(&phone).await, vec![c2.clone(), c3.clone()]);

    // Mute one pinned and one other: the flag is independent of pinning.
    set_pref(&alice, &c1, ConvPrefRequest { mute_until_ms: Some(-1), ..Default::default() }).await;
    let p = set_pref(&alice, &c2, ConvPrefRequest { mute_until_ms: Some(-1), ..Default::default() }).await;
    assert!(p.favourite && p.is_muted(now_ms()), "a muted pinned conversation stays pinned");
    assert_eq!(pinned(&alice).await, vec![c2.clone(), c3.clone()]);
    set_pref(&alice, &c2, ConvPrefRequest { mute_until_ms: Some(0), ..Default::default() }).await;

    // Swap: down then up.
    order(&alice, &[&c3, &c2]).await;
    assert_eq!(pinned(&phone).await, vec![c3.clone(), c2.clone()]);
    order(&alice, &[&c2, &c3]).await;
    assert_eq!(pinned(&alice).await, vec![c2.clone(), c3.clone()]);

    // Unpin: back among the others, the order entry of the others is untouched.
    set_pref(&alice, &c3, ConvPrefRequest { favourite: Some(false), ..Default::default() }).await;
    assert_eq!(pinned(&alice).await, vec![c2.clone()]);
    assert_eq!(pinned(&phone).await, vec![c2.clone()]);
    // The other user's lists are their own.
    assert!(pinned(&bob).await.is_empty());
    // Conversations the user is not in cannot be pinned.
    let secret = s.channel("secret", &[&bob]).await;
    assert!(alice.put::<ConvPref, _>(&format!("/conversations/{secret}/prefs"), &ConvPrefRequest { favourite: Some(true), ..Default::default() }).await.is_err());
}

#[tokio::test]
async fn dms_and_channels_share_one_pinned_order() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let ch = s.channel("general", &[&alice, &bob]).await;
    let dm = alice.open_dm(&bob.user_id).await.unwrap();
    set_pref(&alice, &ch, ConvPrefRequest { favourite: Some(true), ..Default::default() }).await;
    set_pref(&alice, &dm, ConvPrefRequest { favourite: Some(true), ..Default::default() }).await;
    order(&alice, &[&dm, &ch]).await;
    assert_eq!(pinned(&alice).await, vec![dm.clone(), ch.clone()]);
    // Positions are the places in the one list, whatever the conversation kind.
    let ps = prefs(&alice).await;
    let pos = |id: &String| ps.iter().find(|p| &p.conv_id == id).unwrap().position;
    assert_eq!((pos(&dm), pos(&ch)), (Some(0), Some(1)));
    // Re-ordering a subset leaves the others where they are in the person's saved order.
    order(&alice, &[&ch, &dm]).await;
    assert_eq!(pinned(&alice).await, vec![ch, dm]);
}
