//! 只读探针：确认当前登录/歌单接口哪些可用。
//!
//! `cargo test -p lx-source --test wy_endpoint_probe -- --ignored --nocapture`

use lx_core::model::source::SourceId;
use lx_source::session::SessionStore;
use lx_source::wy::{login, playlist as wy_playlist};

fn cookie() -> String {
    let session = SessionStore::load(SourceId::Wy).snapshot();
    session
        .cookie_header_of(&["MUSIC_U", "MUSIC_A", "__csrf", "NMTID"])
        .unwrap_or_default()
}

async fn probe(client: &reqwest::Client, label: &str, url: &str) {
    let mut req = client
        .get(url)
        .header("Referer", "https://music.163.com/")
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; WOW64) AppleWebKit/537.36 Chrome/91.0.4472.164 NeteaseMusicDesktop/3.0.18.203152",
        );
    let c = cookie();
    if !c.is_empty() {
        req = req.header("Cookie", c);
    }
    match req.send().await {
        Ok(response) => {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            let code = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .map(|json| json["code"].to_string())
                .unwrap_or_else(|| "非JSON".into());
            println!(
                "  {label:<44} http={status} code={code:<8} {len}B",
                len = text.len()
            );
        }
        Err(error) => println!("  {label:<44} 请求失败: {error}"),
    }
}

#[tokio::test]
#[ignore = "需要联网，手动执行"]
async fn report_which_endpoints_work() {
    let client = reqwest::Client::new();
    println!("== 会话状态 ==");
    let session = SessionStore::load(SourceId::Wy).snapshot();
    println!("  MUSIC_U 存在: {}", session.has_cookie("MUSIC_U"));
    match login::refresh().await {
        Ok(true) => println!("  login::refresh() -> 会话有效 ✓"),
        Ok(false) => println!("  login::refresh() -> 会话已失效 ✗"),
        Err(error) => println!("  login::refresh() -> 无法判定: {error}"),
    }

    println!("\n== 扫码登录接口（不写入任何东西，只申请一个二维码 key）==");
    match login::create().await {
        Ok(session) => println!(
            "  qrcode/unikey -> 可用 ✓ key 长度 {} 登录链接 {}",
            session.key.len(),
            session.url
        ),
        Err(error) => println!("  qrcode/unikey -> 失败 ✗ {error}"),
    }
    // client/login 需要一个非法 key，看它是否返回「业务码」而不是 404：
    // 只要能返回 800/801/802/803 级别的业务码，就说明轮询接口存在。
    match login::check("voicefox-probe-invalid-key").await {
        Ok(result) => println!("  qrcode/client/login -> 可用 ✓ 状态 {:?}", result.status),
        Err(error) => println!("  qrcode/client/login -> 失败 ✗ {error}"),
    }

    println!("\n== 歌单/账号接口（GET）==");
    probe(
        &client,
        "user/playlist（我的歌单）",
        "https://music.163.com/api/user/playlist?uid=0&limit=1&offset=0",
    )
    .await;
    probe(
        &client,
        "nuser/account/get（账号信息）",
        "https://music.163.com/api/nuser/account/get",
    )
    .await;
    probe(
        &client,
        "v3/playlist/detail（歌单详情）",
        "https://music.163.com/api/v3/playlist/detail?id=5204941757&n=0&s=0",
    )
    .await;
    probe(
        &client,
        "v3/song/detail（批量歌曲详情）",
        "https://music.163.com/api/v3/song/detail?c=%5B%7B%22id%22%3A347230%7D%5D",
    )
    .await;
    probe(
        &client,
        "song/detail（v1 批量）",
        "https://music.163.com/api/song/detail?ids=%5B347230%5D",
    )
    .await;
    probe(
        &client,
        "playlist/list（热门歌单）",
        "https://music.163.com/api/playlist/list?cat=%E5%85%A8%E9%83%A8&order=hot&limit=1&offset=0",
    )
    .await;
    probe(
        &client,
        "playlist/catalogue（分类）",
        "https://music.163.com/api/playlist/catalogue",
    )
    .await;

    println!("\n== 已知不可用的接口（这就是不能做「会话续期」的原因）==");
    probe(
        &client,
        "login/token/refresh",
        "https://music.163.com/api/login/token/refresh",
    )
    .await;
    probe(
        &client,
        "login/refresh",
        "https://music.163.com/api/login/refresh",
    )
    .await;
    probe(
        &client,
        "playlist/track/all",
        "https://music.163.com/api/playlist/track/all?id=5204941757&limit=10&offset=0",
    )
    .await;

    println!("\n== 走音源公开方法 ==");
    match wy_playlist::get_all_user_playlists().await {
        Ok(items) => {
            println!("  get_all_user_playlists() -> {} 个歌单 ✓", items.len());
            assert!(!items.is_empty(), "应当至少取到一个歌单");

            // 回归守卫：收藏来的歌单曾经只返回前 20 首 tracks，完整列表其实在
            // trackIds。只要账号里有超过 20 首的歌单，就不允许只取回 20 首。
            // 声明数量（trackCount）偶尔与真实列表差一两首，所以用容差比较。
            let mut checked = 0;
            for item in items.iter().filter(|item| item.song_count > 20).take(3) {
                match wy_playlist::get_detail(&item.id, 0).await {
                    Ok(songs) => {
                        println!(
                            "  {:<28} 声明 {:>4}  实取 {:>4}",
                            item.name,
                            item.song_count,
                            songs.len()
                        );
                        assert!(
                            songs.len() > 20,
                            "歌单「{}」只取到 {} 首，疑似 20 首截断回归",
                            item.name,
                            songs.len()
                        );
                        let drift = songs.len() as i64 - item.song_count as i64;
                        assert!(
                            drift.abs() <= 2,
                            "歌单「{}」实取 {} 首 vs 声明 {} 首，差距过大",
                            item.name,
                            songs.len(),
                            item.song_count
                        );
                        checked += 1;
                    }
                    Err(error) => println!("  {:<28} 失败: {error}", item.name),
                }
            }
            println!("  已逐歌单校验 {checked} 个（>20 首的）歌单");
        }
        Err(error) => panic!("get_all_user_playlists() 失败: {error}"),
    }
}
