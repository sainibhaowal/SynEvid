import { AccountSummary } from '../core/contracts';

export class DatabaseDriver {
    private table: Map<string, AccountSummary> = new Map();

    public queryAccount(accNum: string): AccountSummary | undefined {
        return this.table.get(accNum);
    }

    public updateBalance(accNum: string, newBalance: number): boolean {
        const item = this.table.get(accNum);
        if (item) {
            item.balance = newBalance;
            return true;
        }
        return false;
    }
}
