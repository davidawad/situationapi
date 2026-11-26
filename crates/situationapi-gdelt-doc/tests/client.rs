use situationapi_gdelt_doc::Client;

#[test]
fn builds_synthetic_article_list_url() {
    let url = Client::article_list_url("news", "20261101000000", "20261101235959", 250)
        .expect("DOC URL builds");
    assert!(url.contains("mode=artlist"));
    assert!(url.contains("startdatetime=20261101000000"));
    assert!(url.contains("maxrecords=250"));
}
