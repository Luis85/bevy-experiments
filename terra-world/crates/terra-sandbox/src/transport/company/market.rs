//! Nearby warehouse market trades use company money and real shop inventories.
//! A remote quote cannot grant reach, change prices or consume reserved space.
use super::*;

struct PreparedTrade {
    quote: CompanyDepotTradeQuote,
    warehouse: Inventory,
    shop_stock: Inventory,
    company_cash: u64,
    shop_cash: u64,
    accounting: CompanyAccounting,
    revenue: u64,
}

impl SandboxState {
    pub fn quote_company_depot_trade(
        &self,
        owner: u64,
        facility: u64,
        shop_id: u64,
        item: &str,
        amount: u32,
        buy: bool,
    ) -> Result<CompanyDepotTradeQuote, SandboxError> {
        Ok(self
            .prepare_company_depot_trade(owner, facility, shop_id, item, amount, buy)?
            .quote)
    }

    fn prepare_company_depot_trade(
        &self,
        owner: u64,
        facility: u64,
        shop_id: u64,
        item: &str,
        amount: u32,
        buy: bool,
    ) -> Result<PreparedTrade, SandboxError> {
        if self.is_player_dormant(owner) {
            return Err(invalid("resume the company owner before trading"));
        }
        self.require_company_depot_reach(owner, facility)?;
        let account = self
            .living
            .accounts
            .get(&owner)
            .ok_or(SandboxError::PlayerMissing)?;
        if account.active_unit != account.avatar_unit {
            return Err(invalid("leave the vehicle before trading depot goods"));
        }
        if !(1..=100).contains(&amount) || item.is_empty() || item.len() > 64 {
            return Err(invalid("depot trade item or quantity (1..100)"));
        }
        let stock = self.company_depot_stock(owner, facility)?;
        let company = &self.transport.companies[&owner];
        let shop = self
            .living
            .shops
            .get(&shop_id)
            .ok_or_else(|| invalid("shop"))?;
        if !near(self.players[&owner].position, shop.position) {
            return Err(SandboxError::OutOfReach);
        }
        let price = shop
            .prices
            .get(item)
            .ok_or_else(|| invalid("shop does not trade this item"))?;
        let unit_price = if buy { price.buy } else { price.sell };
        let total = unit_price
            .checked_mul(u64::from(amount))
            .ok_or(SandboxError::Capacity)?;
        let goods = inventory(&[(item, amount)]);
        let (warehouse, shop_stock, company_cash, shop_cash) = if buy {
            (
                exchange(stock, &Inventory::new(), &goods, 1)?,
                exchange(&shop.stock, &goods, &Inventory::new(), 1)?,
                company
                    .cash
                    .checked_sub(total)
                    .ok_or_else(|| SandboxError::Insufficient("company purchase funds".into()))?,
                shop.money
                    .checked_add(total)
                    .filter(|n| *n <= MAX_MONEY)
                    .ok_or(SandboxError::Capacity)?,
            )
        } else {
            (
                exchange(stock, &goods, &Inventory::new(), 1)?,
                exchange(&shop.stock, &Inventory::new(), &goods, 1)?,
                company
                    .cash
                    .checked_add(total)
                    .filter(|n| *n <= MAX_MONEY)
                    .ok_or(SandboxError::Capacity)?,
                shop.money
                    .checked_sub(total)
                    .ok_or_else(|| SandboxError::Insufficient("shop purchase funds".into()))?,
            )
        };
        self.validate_company_depot_stock(owner, facility, &warehouse, None)?;
        let accounting = company.prepare_accounting(
            self.transport.elapsed_seconds,
            &[(
                if buy {
                    CompanyCashCategory::InventoryPurchases
                } else {
                    CompanyCashCategory::GoodsSales
                },
                total,
            )],
        )?;
        let revenue = company
            .revenue
            .checked_add(if buy { 0 } else { total })
            .filter(|n| *n <= MAX_COMPANY_ACCOUNTING_TOTAL)
            .ok_or(SandboxError::Capacity)?;
        Ok(PreparedTrade {
            quote: CompanyDepotTradeQuote {
                facility_id: facility,
                shop_id,
                item: item.into(),
                amount,
                buy,
                unit_price,
                total_cost: total,
            },
            warehouse,
            shop_stock,
            company_cash,
            shop_cash,
            accounting,
            revenue,
        })
    }

    pub(super) fn trade_company_depot_stock(
        &mut self,
        owner: u64,
        facility: u64,
        shop_id: u64,
        item: &str,
        amount: u32,
        buy: bool,
        quoted_total: u64,
        catalog: &Catalog,
    ) -> Result<Outcome, SandboxError> {
        if !catalog.items.contains(item) {
            return Err(invalid("depot catalog item"));
        }
        let trade =
            self.prepare_company_depot_trade(owner, facility, shop_id, item, amount, buy)?;
        if trade.quote.total_cost != quoted_total {
            return Err(invalid("shop quote changed; review the updated total"));
        }
        *self.company_depot_stock_mut(owner, facility)? = trade.warehouse;
        let company = self.transport.companies.get_mut(&owner).unwrap();
        company.cash = trade.company_cash;
        company.accounting = Some(trade.accounting);
        company.revenue = trade.revenue;
        let shop = self.living.shops.get_mut(&shop_id).unwrap();
        shop.stock = trade.shop_stock;
        shop.money = trade.shop_cash;
        self.living.metrics.units_traded = self
            .living
            .metrics
            .units_traded
            .saturating_add(u64::from(amount));
        self.living.metrics.credits_traded = self
            .living
            .metrics
            .credits_traded
            .saturating_add(quoted_total);
        Ok(outcome(&format!(
            "{} {amount} {item} {} company depot for {}.{:02} company credits",
            if buy { "Bought" } else { "Sold" },
            if buy { "into" } else { "from" },
            quoted_total / 100,
            quoted_total % 100
        )))
    }
}
