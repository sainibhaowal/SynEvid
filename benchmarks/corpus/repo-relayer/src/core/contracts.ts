export interface AccountSummary {
    accountNumber: string;
    balance: number;
    currency: string;
}

export interface TransactionRequest {
    fromAccount: string;
    toAccount: string;
    amount: number;
}
