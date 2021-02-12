use crate::parser::*;
use crate::types::{Asset, Auction, LotAuctionKind, Management};
use std::collections::HashMap;

pub(crate) const BASE_BOE_URL: &str = "https://subastas.boe.es/";

static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

lazy_static! {
    static ref MAIN_AUCTION_BOE_URL: String = BASE_BOE_URL.to_owned() + "/subastas_ava.php?campo%5B0%5D=SUBASTA.ORIGEN&dato%5B0%5D=&campo%5B1%5D=SUBASTA.ESTADO&dato%5B1%5D=EJ&campo%5B2%5D=BIEN.TIPO&dato%5B2%5D=&dato%5B3%5D=&campo%5B4%5D=BIEN.DIRECCION&dato%5B4%5D=&campo%5B5%5D=BIEN.CODPOSTAL&dato%5B5%5D=&campo%5B6%5D=BIEN.LOCALIDAD&dato%5B6%5D=&campo%5B7%5D=BIEN.COD_PROVINCIA&dato%5B7%5D=&campo%5B8%5D=SUBASTA.POSTURA_MINIMA_MINIMA_LOTES&dato%5B8%5D=&campo%5B9%5D=SUBASTA.NUM_CUENTA_EXPEDIENTE_1&dato%5B9%5D=&campo%5B10%5D=SUBASTA.NUM_CUENTA_EXPEDIENTE_2&dato%5B10%5D=&campo%5B11%5D=SUBASTA.NUM_CUENTA_EXPEDIENTE_3&dato%5B11%5D=&campo%5B12%5D=SUBASTA.NUM_CUENTA_EXPEDIENTE_4&dato%5B12%5D=&campo%5B13%5D=SUBASTA.NUM_CUENTA_EXPEDIENTE_5&dato%5B13%5D=&campo%5B14%5D=SUBASTA.ID_SUBASTA_BUSCAR&dato%5B14%5D=&campo%5B15%5D=SUBASTA.FECHA_FIN_YMD&dato%5B15%5D%5B0%5D=&dato%5B15%5D%5B1%5D=&campo%5B16%5D=SUBASTA.FECHA_INICIO_YMD&dato%5B16%5D%5B0%5D=&dato%5B16%5D%5B1%5D=&page_hits=500&sort_field%5B0%5D=SUBASTA.FECHA_FIN_YMD&sort_order%5B0%5D=asc&sort_field%5B1%5D=SUBASTA.FECHA_FIN_YMD&sort_order%5B1%5D=asc&sort_field%5B2%5D=SUBASTA.HORA_FIN&sort_order%5B2%5D=asc&accion=Buscar";
}

pub(crate) fn get_url(target: &str) -> Result<String, Box<dyn std::error::Error>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent(APP_USER_AGENT)
        .build()?;

    let body = client.get(target).send()?.error_for_status()?.text()?;

    Ok(body)
}

fn get_auctions_links() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let main_page = get_url(&MAIN_AUCTION_BOE_URL)?;
    let mut result = parse_result_page(&main_page)?;

    for page_url in parse_extra_pages(&main_page)? {
        let extra_page = get_url(&page_url)?;
        result.append(&mut parse_result_page(&extra_page)?);
    }

    Ok(result)
}

fn process_auction_link(link: &str) -> Result<(Auction, Vec<Asset>), Box<dyn std::error::Error>> {
    let mut assets = Vec::new();

    let auction_page = get_url(link)?;

    let (mgm_link, asset_link) = parse_main_auction_links(&auction_page)?;
    let management_page = get_url(&mgm_link)?;
    let management = Management::new(&parse_management_auction_page(&management_page)?);
    log::info!("Created management: {}", management.code);

    let auction = Auction::new(&parse_main_auction_page(&auction_page)?, management);
    log::info!("Created auction: {}", auction.id);

    let asset_page = get_url(&asset_link)?;
    match auction.lot_kind {
        LotAuctionKind::NotApplicable => {
            let asset = Asset::new(&auction.id, &parse_asset_auction_page(&asset_page)?);
            assets.push(asset);
        }
        LotAuctionKind::Joined | LotAuctionKind::Splitted => {
            let lot_links = parse_lot_auction_page_links(&asset_page)?;
            for (i, lot_link) in lot_links.iter().enumerate() {
                let lot_page = get_url(&lot_link)?;
                let asset = Asset::new(&auction.id, &parse_lot_auction_page(&lot_page, i + 1)?);
                assets.push(asset);
            }
        }
    }

    Ok((auction, assets))
}

/// Scrape subastas.boe.es to get all assets in auctions.
pub fn scrape() -> Result<(HashMap<String, Auction>, Vec<Asset>), Box<dyn std::error::Error>> {
    let mut total_assets = Vec::new();
    let mut total_auctions = HashMap::new();
    let auction_links = get_auctions_links()?;
    let number_auctions = auction_links.len();

    log::info!("Total auctions to process: {}", number_auctions);
    let mut auction_ok = 0;
    let mut auction_err = 0;

    for auction_link in auction_links {
        match process_auction_link(&auction_link) {
            Ok((auction, mut auction_assets)) => {
                auction_ok += 1;
                total_auctions.insert(auction.id.clone(), auction);
                total_assets.append(&mut auction_assets)
            }
            Err(err) => {
                auction_err += 1;
                log::warn!("Unable to process: {}", err)
            }
        }
        log::info!(
            "Auctions processed: {}/{}, Auctions errors: {}",
            auction_ok + auction_err,
            number_auctions,
            auction_err
        );
    }

    Ok((total_auctions, total_assets))
}
