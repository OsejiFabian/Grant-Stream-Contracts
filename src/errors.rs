use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    // // General (100s)
    Unauthorized = 100,
    InvalidInput = 101,

    // Grant (200s)
    GrantNotFound = 200,
    NothingToClaim = 206,
    AlreadyFullyClaimed = 207,
    GrantExpired = 208,
    DurationExceedsMaximum = 209,

    // Financial (300s)
    InsufficientBalance = 300,

    // Compliance (400s)
    KycNotCompleted = 400,
    KycExpired = 401,
    AddressSanctioned = 402,
    JurisdictionRestricted = 403,
    LegalSignatureMissing = 404,
    LegalSignatureInvalid = 405,
    ComplianceCheckFailed = 406,
    AmlThresholdExceeded = 407,
    RiskRatingTooHigh = 408,
    DocumentVerificationFailed = 409,
    AccreditationStatusInvalid = 410,
    TaxComplianceFailed = 411,
    RegulatoryBlockActive = 412,
    WhitelistNotApproved = 413,
    BlacklistViolation = 414,
    GeofencingRestriction = 415,
    IdentityVerificationExpired = 416,
    SourceOfFundsNotVerified = 417,
    BeneficialOwnerNotVerified = 418,
    PoliticallyExposedPerson = 419,
    SanctionsListHit = 420,

    // System (900s)
    Overflow = 900,
}
