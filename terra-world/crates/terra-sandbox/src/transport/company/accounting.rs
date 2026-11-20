//! Cash-basis business records. These are receipts, never spendable accounts or
//! inventory valuation. An imported opening balance cannot reconstruct expenses.
use super::*;

pub const MAX_COMPANY_RECENT_ENTRIES: usize = 64;
/// Exact lifetime totals stop accepting new value at this bound; never saturate.
pub const MAX_COMPANY_ACCOUNTING_TOTAL: u64 = 1_000_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompanyAccountingCoverage {
    SinceRegistration,
    ImportedOpeningBalance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompanyCashClass {
    Operating,
    Capital,
    Financing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompanyCashCategory {
    PublicFares,
    GoodsSales,
    InventoryPurchases,
    Payroll,
    Recruitment,
    Fuel,
    Repairs,
    ServiceCallout,
    PublicServiceContribution,
    DepotAccess,
    OwnerDeposit,
    OwnerWithdrawal,
    OwnerPaidRegistration,
    OwnerPaidLease,
    OwnerPaidFuel,
    OwnerPaidRepair,
    OwnerPaidUpgrade,
    OwnerPaidPaint,
}

impl CompanyCashCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::PublicFares => "Public passenger and freight fares",
            Self::GoodsSales => "Goods sales",
            Self::InventoryPurchases => "Stock purchases",
            Self::Payroll => "Employee payroll",
            Self::Recruitment => "Employee recruitment",
            Self::Fuel => "Fuel",
            Self::Repairs => "Repairs",
            Self::ServiceCallout => "Roadside callout",
            Self::PublicServiceContribution => "Public service contributions",
            Self::DepotAccess => "Depot access",
            Self::OwnerDeposit => "Owner deposits",
            Self::OwnerWithdrawal => "Owner withdrawals",
            Self::OwnerPaidRegistration => "Owner-paid registration",
            Self::OwnerPaidLease => "Owner-paid assigned-vehicle lease",
            Self::OwnerPaidFuel => "Owner-paid assigned-vehicle fuel",
            Self::OwnerPaidRepair => "Owner-paid assigned-vehicle repair",
            Self::OwnerPaidUpgrade => "Owner-paid assigned-vehicle upgrade",
            Self::OwnerPaidPaint => "Owner-paid assigned-vehicle paint",
        }
    }

    pub fn class(self) -> CompanyCashClass {
        match self {
            Self::DepotAccess | Self::OwnerPaidRegistration | Self::OwnerPaidUpgrade => {
                CompanyCashClass::Capital
            }
            Self::OwnerDeposit | Self::OwnerWithdrawal => CompanyCashClass::Financing,
            _ => CompanyCashClass::Operating,
        }
    }

    pub fn owner_paid(self) -> bool {
        matches!(
            self,
            Self::OwnerPaidRegistration
                | Self::OwnerPaidLease
                | Self::OwnerPaidFuel
                | Self::OwnerPaidRepair
                | Self::OwnerPaidUpgrade
                | Self::OwnerPaidPaint
        )
    }

    pub fn receipt(self) -> bool {
        matches!(
            self,
            Self::PublicFares | Self::GoodsSales | Self::OwnerDeposit
        )
    }

    fn cash_delta(self, amount: u64) -> i64 {
        if self.owner_paid() {
            0
        } else if self.receipt() {
            amount as i64
        } else {
            -(amount as i64)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompanyCashEntry {
    pub sequence: u64,
    pub elapsed_seconds: f64,
    pub category: CompanyCashCategory,
    pub amount: u64,
    /// Owner-paid receipts do not change this company-account balance.
    pub cash_after: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompanyAccounting {
    pub coverage: CompanyAccountingCoverage,
    pub recorded_since_elapsed_seconds: f64,
    pub opening_cash: u64,
    /// Historical counters are retained, not reclassified as recorded receipts.
    pub opening_revenue: u64,
    pub opening_payroll: u64,
    pub totals: BTreeMap<CompanyCashCategory, u64>,
    pub monetary_entry_count: u64,
    pub recent_entries: Vec<CompanyCashEntry>,
    /// Physical owner transfers have no price/cost-basis authority. These counts
    /// disclose that omission without inventing income or an acquisition price.
    pub unpriced_stock_in_units: u64,
    pub unpriced_stock_out_units: u64,
    pub personal_vehicle_used: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanyCashFlow {
    pub receipts: u64,
    pub payments: u64,
}

impl CompanyCashFlow {
    pub fn net(self) -> i64 {
        self.receipts as i64 - self.payments as i64
    }
}

/// An owner-private cash report, not accrual profit or an asset valuation.
/// Monetary fields use minor credits. Totals cover the stated recording period,
/// including entries that have aged out of the bounded recent journal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompanyAccountingReport {
    pub coverage: CompanyAccountingCoverage,
    /// None means the legacy company has not recorded its first new event yet.
    pub recorded_since_elapsed_seconds: Option<f64>,
    pub opening_cash: u64,
    pub opening_revenue: u64,
    pub opening_payroll: u64,
    pub cash: u64,
    pub expected_cash: i64,
    pub reconciled: bool,
    pub operating: CompanyCashFlow,
    pub capital: CompanyCashFlow,
    pub financing: CompanyCashFlow,
    pub owner_paid_operating: u64,
    pub owner_paid_capital: u64,
    /// Owner deposits plus attributable personal payments, before withdrawals.
    pub known_owner_funding: u64,
    pub cash_result_before_financing: i64,
    pub cash_result_after_known_owner_costs: i64,
    pub totals: BTreeMap<CompanyCashCategory, u64>,
    pub monetary_entry_count: u64,
    pub recent_entries: Vec<CompanyCashEntry>,
    pub unpriced_stock_in_units: u64,
    pub unpriced_stock_out_units: u64,
    pub personal_vehicle_used: bool,
}

impl CompanyAccounting {
    pub(super) fn since_registration(elapsed: f64) -> Self {
        Self {
            coverage: CompanyAccountingCoverage::SinceRegistration,
            recorded_since_elapsed_seconds: elapsed,
            opening_cash: 0,
            opening_revenue: 0,
            opening_payroll: 0,
            totals: BTreeMap::new(),
            monetary_entry_count: 0,
            recent_entries: Vec::new(),
            unpriced_stock_in_units: 0,
            unpriced_stock_out_units: 0,
            personal_vehicle_used: false,
        }
    }

    fn imported(company: &TransportCompany, elapsed: f64) -> Self {
        Self {
            coverage: CompanyAccountingCoverage::ImportedOpeningBalance,
            opening_cash: company.cash,
            opening_revenue: company.revenue,
            opening_payroll: company.payroll_paid(),
            personal_vehicle_used: company.drivers.iter().any(|d| d.vehicle_id.is_some()),
            ..Self::since_registration(elapsed)
        }
    }

    pub fn total(&self, category: CompanyCashCategory) -> u64 {
        self.totals.get(&category).copied().unwrap_or(0)
    }

    fn expected_cash(&self) -> i64 {
        self.opening_cash as i64
            + self
                .totals
                .iter()
                .map(|(&category, &amount)| category.cash_delta(amount))
                .sum::<i64>()
    }

    fn append(
        &mut self,
        elapsed: f64,
        category: CompanyCashCategory,
        amount: u64,
    ) -> Result<(), SandboxError> {
        if amount == 0 {
            return Ok(());
        }
        if amount > MAX_MONEY
            || !elapsed.is_finite()
            || elapsed < self.recorded_since_elapsed_seconds
            || self
                .recent_entries
                .last()
                .is_some_and(|entry| entry.elapsed_seconds > elapsed)
        {
            return Err(invalid("company accounting entry amount or time"));
        }
        let total = self
            .total(category)
            .checked_add(amount)
            .filter(|n| *n <= MAX_COMPANY_ACCOUNTING_TOTAL)
            .ok_or(SandboxError::Capacity)?;
        let sequence = self
            .monetary_entry_count
            .checked_add(1)
            .filter(|n| *n <= MAX_COMPANY_ACCOUNTING_TOTAL)
            .ok_or(SandboxError::Capacity)?;
        let cash = self.expected_cash() + category.cash_delta(amount);
        let cash_after = u64::try_from(cash)
            .ok()
            .filter(|n| *n <= MAX_MONEY)
            .ok_or(SandboxError::Capacity)?;
        self.totals.insert(category, total);
        self.monetary_entry_count = sequence;
        if self.recent_entries.len() == MAX_COMPANY_RECENT_ENTRIES {
            self.recent_entries.remove(0);
        }
        self.recent_entries.push(CompanyCashEntry {
            sequence,
            elapsed_seconds: elapsed,
            category,
            amount,
            cash_after,
        });
        Ok(())
    }

    pub(super) fn validate(
        &self,
        company: &TransportCompany,
        elapsed: f64,
    ) -> Result<(), SandboxError> {
        if !finite_range(self.recorded_since_elapsed_seconds, 0., elapsed)
            || self.opening_cash > MAX_MONEY
            || self.opening_revenue > MAX_COMPANY_ACCOUNTING_TOTAL
            || self.opening_payroll > 10 * MAX_COMPANY_ACCOUNTING_TOTAL
            || self.monetary_entry_count > MAX_COMPANY_ACCOUNTING_TOTAL
            || self.unpriced_stock_in_units > MAX_COMPANY_ACCOUNTING_TOTAL
            || self.unpriced_stock_out_units > MAX_COMPANY_ACCOUNTING_TOTAL
            || self
                .totals
                .values()
                .any(|n| *n == 0 || *n > MAX_COMPANY_ACCOUNTING_TOTAL)
            || self.recent_entries.len()
                != self
                    .monetary_entry_count
                    .min(MAX_COMPANY_RECENT_ENTRIES as u64) as usize
        {
            return Err(invalid("company accounting bounds"));
        }
        let total_amount: u64 = self.totals.values().sum();
        if total_amount < self.monetary_entry_count
            || u128::from(total_amount)
                > u128::from(self.monetary_entry_count) * u128::from(MAX_MONEY)
        {
            return Err(invalid("company accounting event count or amount"));
        }
        if self.coverage == CompanyAccountingCoverage::SinceRegistration
            && (self.opening_cash != 0
                || self.opening_revenue != 0
                || self.opening_payroll != 0
                || self.total(CompanyCashCategory::OwnerPaidRegistration)
                    != COMPANY_REGISTRATION_PRICE)
        {
            return Err(invalid("company accounting registration origin"));
        }
        if self.expected_cash() != company.cash as i64
            || self.opening_revenue
                + self.total(CompanyCashCategory::PublicFares)
                + self.total(CompanyCashCategory::GoodsSales)
                != company.revenue
            || self.opening_payroll + self.total(CompanyCashCategory::Payroll)
                != company.payroll_paid()
        {
            return Err(invalid(
                "company cash, revenue or payroll does not reconcile",
            ));
        }
        let mut remaining = self.totals.clone();
        let mut cash = company.cash as i64;
        let mut next_time = elapsed;
        for (offset, entry) in self.recent_entries.iter().rev().enumerate() {
            if entry.sequence != self.monetary_entry_count - offset as u64
                || entry.amount == 0
                || entry.amount > MAX_MONEY
                || entry.cash_after > MAX_MONEY
                || entry.cash_after as i64 != cash
                || !finite_range(
                    entry.elapsed_seconds,
                    self.recorded_since_elapsed_seconds,
                    next_time,
                )
            {
                return Err(invalid(
                    "company recent accounting sequence, time or balance",
                ));
            }
            let total = remaining
                .get_mut(&entry.category)
                .ok_or_else(|| invalid("company recent accounting category"))?;
            *total = total
                .checked_sub(entry.amount)
                .ok_or_else(|| invalid("company recent accounting exceeds lifetime total"))?;
            cash -= entry.category.cash_delta(entry.amount);
            if !(0..=MAX_MONEY as i64).contains(&cash) {
                return Err(invalid("company recent accounting prior cash"));
            }
            next_time = entry.elapsed_seconds;
        }
        if self.monetary_entry_count <= MAX_COMPANY_RECENT_ENTRIES as u64
            && (remaining.values().any(|n| *n != 0) || cash != self.opening_cash as i64)
        {
            return Err(invalid(
                "company complete recent journal omits recorded value",
            ));
        }
        let archived_count = self.monetary_entry_count - self.recent_entries.len() as u64;
        let archived_amount: u64 = remaining.values().sum();
        if archived_amount < archived_count
            || u128::from(archived_amount) > u128::from(archived_count) * u128::from(MAX_MONEY)
        {
            return Err(invalid("company archived accounting event count or amount"));
        }
        Ok(())
    }
}

impl TransportCompany {
    /// Records are prepared before any cash, stock, escrow or employee mutation.
    /// Legacy companies begin from their retained current balances, never zero.
    pub(super) fn prepare_accounting(
        &self,
        elapsed: f64,
        entries: &[(CompanyCashCategory, u64)],
    ) -> Result<CompanyAccounting, SandboxError> {
        let mut accounting = self
            .accounting
            .clone()
            .unwrap_or_else(|| CompanyAccounting::imported(self, elapsed));
        if accounting.expected_cash() != self.cash as i64 {
            return Err(invalid("company opening cash does not reconcile"));
        }
        for &(category, amount) in entries {
            accounting.append(elapsed, category, amount)?;
        }
        if accounting.opening_revenue
            + accounting.total(CompanyCashCategory::PublicFares)
            + accounting.total(CompanyCashCategory::GoodsSales)
            > MAX_COMPANY_ACCOUNTING_TOTAL
        {
            return Err(SandboxError::Capacity);
        }
        Ok(accounting)
    }

    pub fn accounting_report(&self) -> CompanyAccountingReport {
        let accounting = self
            .accounting
            .clone()
            .unwrap_or_else(|| CompanyAccounting::imported(self, 0.));
        let mut operating = CompanyCashFlow::default();
        let mut capital = CompanyCashFlow::default();
        let mut financing = CompanyCashFlow::default();
        let mut owner_paid_operating = 0;
        let mut owner_paid_capital = 0;
        for (&category, &amount) in &accounting.totals {
            if category.owner_paid() {
                if category.class() == CompanyCashClass::Capital {
                    owner_paid_capital += amount;
                } else {
                    owner_paid_operating += amount;
                }
                continue;
            }
            let flow = match category.class() {
                CompanyCashClass::Operating => &mut operating,
                CompanyCashClass::Capital => &mut capital,
                CompanyCashClass::Financing => &mut financing,
            };
            if category.receipt() {
                flow.receipts += amount;
            } else {
                flow.payments += amount;
            }
        }
        let expected_cash = accounting.expected_cash();
        let cash_result_before_financing = operating.net() + capital.net();
        CompanyAccountingReport {
            coverage: accounting.coverage,
            recorded_since_elapsed_seconds: self
                .accounting
                .as_ref()
                .map(|a| a.recorded_since_elapsed_seconds),
            opening_cash: accounting.opening_cash,
            opening_revenue: accounting.opening_revenue,
            opening_payroll: accounting.opening_payroll,
            cash: self.cash,
            expected_cash,
            reconciled: expected_cash == self.cash as i64,
            operating,
            capital,
            financing,
            owner_paid_operating,
            owner_paid_capital,
            known_owner_funding: financing.receipts + owner_paid_operating + owner_paid_capital,
            cash_result_before_financing,
            cash_result_after_known_owner_costs: cash_result_before_financing
                - owner_paid_operating as i64
                - owner_paid_capital as i64,
            totals: accounting.totals,
            monetary_entry_count: accounting.monetary_entry_count,
            recent_entries: accounting.recent_entries,
            unpriced_stock_in_units: accounting.unpriced_stock_in_units,
            unpriced_stock_out_units: accounting.unpriced_stock_out_units,
            personal_vehicle_used: accounting.personal_vehicle_used,
        }
    }
}

impl SandboxState {
    /// Charge an owner-paid vehicle service and commit its optional receipt as
    /// one transaction. Only spending while assigned has a defensible company
    /// allocation; earlier personal acquisition is never retroactively charged.
    pub(in crate::transport) fn charge_owner_paid_company_vehicle(
        &mut self,
        owner: u64,
        vehicle: u64,
        category: CompanyCashCategory,
        amount: u64,
    ) -> Result<(), SandboxError> {
        if !category.owner_paid() || category == CompanyCashCategory::OwnerPaidRegistration {
            return Err(invalid("owner-paid company vehicle expense category"));
        }
        // Prepare the optional receipt before touching either account. A
        // rejected wallet/treasury charge must not leave a phantom expense,
        // while an exhausted journal must not debit an otherwise valid payment.
        let accounting = self
            .transport
            .companies
            .get(&owner)
            .filter(|company| {
                company
                    .drivers
                    .iter()
                    .any(|d| d.vehicle_id == Some(vehicle))
            })
            .map(|company| {
                let mut accounting = company
                    .prepare_accounting(self.transport.elapsed_seconds, &[(category, amount)])?;
                accounting.personal_vehicle_used = true;
                Ok::<_, SandboxError>(accounting)
            })
            .transpose()?;
        self.transport_charge(owner, amount)?;
        if let Some(accounting) = accounting {
            self.transport.companies.get_mut(&owner).unwrap().accounting = Some(accounting);
        }
        Ok(())
    }
}
