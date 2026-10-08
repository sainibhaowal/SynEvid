import { BankingDomain } from '../domain/businessLogic';
import { TransactionRequest } from '../core/contracts';
import { sanitizeInput } from '../security/sanitizer';

export class BankingView {
    constructor(private domain: BankingDomain) {}

    public renderTransferForm(from: string, to: string, amountStr: string): string {
        const cleanFrom = sanitizeInput(from);
        const cleanTo = sanitizeInput(to);
        const amount = parseFloat(amountStr);

        const req: TransactionRequest = {
            fromAccount: cleanFrom,
            toAccount: cleanTo,
            amount,
        };

        const success = this.domain.processTransfer(req);
        return success ? "Transfer completed successfully." : "Transfer failed.";
    }
}
