use ::entity::{notification_preferences, price_history, products};
use prelude::Decimal;
// price_scraper.rs
use sea_orm::*;
use std::sync::Arc;

use super::myntra::scrape_products;

pub struct PriceScraper {
    db: Arc<DatabaseConnection>,
}

impl PriceScraper {
    pub fn new(db: DatabaseConnection) -> Self {
        Self {
            db: Arc::new(db)
        }
    }

    pub async fn start_scraping(&self) {
        let db = self.db.clone();
        
        tokio::spawn(async move {
            loop {
                if let Ok(preferences) = notification_preferences::Entity::find().all(&*db).await {
                    let product_ids: Vec<i32> = preferences
                        .iter()
                        .map(|pref| pref.product_id)
                        .collect();
                
                    match scrape_products(product_ids).await.map_err(|e| e.to_string()) {
                        Ok(prices) => {
                            for (pref, price) in preferences.iter().zip(prices) {
                                let decimal_price = Decimal::new(price as i64, 2);
                                update_prices(&db, pref.product_id, decimal_price).await;
                            }
                        }
                        Err(e) => eprintln!("Scraping error: {}", e),
                    }
                }
                tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            }
        });
    }
}

async fn update_prices(db: &DatabaseConnection, product_id: i32, price: Decimal) {
    let history = price_history::ActiveModel {
        product_id: Set(product_id),
        price: Set(price),
        recorded_at: Set(chrono::Utc::now().naive_utc()),
        ..Default::default()
    };
 
    if let Err(e) = history.insert(db).await {
        eprintln!("Failed to insert price history: {}", e);
        return;
    }
 
    if let Ok(Some(current_product)) = products::Entity::find_by_id(product_id).one(db).await {
        let mut product_update: products::ActiveModel = current_product.clone().into();
        product_update.current_price = Set(price);
        product_update.last_updated = Set(chrono::Utc::now().naive_utc());
 
        if price > current_product.highest_price {
            product_update.highest_price = Set(price);
        }
        if price < current_product.lowest_price {
            product_update.lowest_price = Set(price);
        }
 
        if let Err(e) = product_update.update(db).await {
            eprintln!("Failed to update product: {}", e);
        }
    }
 }