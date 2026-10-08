import { DatabaseDriver } from '../infrastructure/database';
import { TransactionRequest } from '../core/contracts';
import { logInfo, logError } from '../shared/logger';

export class BankingDomain {
    constructor(private db: DatabaseDriver) {}

    public processTransfer(req: TransactionRequest): boolean {
        logInfo(`Initiating transfer from ${req.fromAccount}`);
        const sender = this.db.queryAccount(req.fromAccount);
        const recipient = this.db.queryAccount(req.toAccount);

        if (!sender || !recipient || sender.balance < req.amount) {
            logError("Insufficient funds or invalid accounts");
            return false;
        }

        this.db.updateBalance(req.fromAccount, sender.balance - req.amount);
        this.db.updateBalance(req.toAccount, recipient.balance + req.amount);
        return true;
    }
}
