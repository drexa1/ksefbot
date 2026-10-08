import {WorkflowEntrypoint, WorkflowEvent, WorkflowStep, WorkflowDelayFunction} from "cloudflare:workers";
import {Env} from "../worker";
import {AppUser} from "../types/users";
import {InvoicesBackfill} from "../types/invoices";
import {D1Driver, Repository} from "../repository/d1";
import {KsefClient} from "../clients/ksef";
import {getByDatesRange} from "../routes/ksef/common";
import DateTimeFormat = Intl.DateTimeFormat;

const dateFormat = new DateTimeFormat("en", { month: "long", year: "numeric", timeZone: "UTC" });

let repo: Repository;
const getRepo = (env: Env) => repo ??= new Repository(new D1Driver(env.D1));

export class InvoicesBackfillJob extends WorkflowEntrypoint<Env, { userId: string }> {

    async run(event: Readonly<WorkflowEvent<{ userId: string }>>, step: WorkflowStep): Promise<void> {
        const { userId } = event.payload;
        const appUser = await getRepo(this.env).get<AppUser>("users", { id: userId });
        const client = new KsefClient(this.env);
        // Change job to running
        await this.updateJob(userId, { status: "running" });

        // Backwards from now, the current ongoing year, and the previous one
        let toDate = new Date(event.timestamp.getTime());
        const historyStart = new Date(Date.UTC(toDate.getUTCFullYear() - 1, 0, 1));

        let windowStep = 0;
        let totalDownloaded = 0;
        while (toDate >= historyStart) {
            const fromDate = new Date(toDate);
            fromDate.setUTCHours(0, 0, 0, 0);
            fromDate.setUTCDate(fromDate.getUTCDate() - this.env.KSEF_MAX_DATES_RANGE + 1);
            if (fromDate < historyStart)
                fromDate.setTime(historyStart.getTime());
            for (const type of ["sales", "purchase"] as const) {
                const stepName = `invoices-${type}-${dateFormat.format(fromDate).replace(" ", "")}-${dateFormat.format(toDate).replace(" ", "")}`;
                const downloadedFromKsef = await step.do(stepName, {
                    retries: {
                        limit: 3,
                        delay: this.delay()
                    }
                }, async () => {
                    const appInvoices = await this.getBackfillInvoices(client, userId, appUser, type, fromDate, toDate);
                    console.info(`⏪ [${type}] Backfill step (${windowStep}) from ${fromDate.toISOString()} to ${toDate.toISOString()}: ${appInvoices.fromKsef} invoices`);
                    return appInvoices.fromKsef;
                });
                totalDownloaded += downloadedFromKsef;
                await this.updateJob(userId, { invoicesDownloaded: totalDownloaded });
            }
            toDate = new Date(fromDate);
            toDate.setUTCDate(toDate.getUTCDate() - 1);
            toDate.setUTCHours(23, 59, 59, 999);
            windowStep += 1;
        }
        await this.updateJob(userId, { status: "completed", invoicesDownloaded: totalDownloaded });
    }

    async getBackfillInvoices(client: KsefClient, userId: string, appUser: AppUser, type: "sales" | "purchase", fromDate: Date, toDate: Date) {
        await this.updateJob(userId, { status: "running" });
        try {
            return await getByDatesRange(this.env, client, type, appUser, fromDate, toDate);
        } catch (error) {
            if (String(error).includes("429"))
                await this.updateJob(userId, { status: "throttled", error: String(error) });
            throw error;
        }
    }

    async updateJob(ownerId: string, data: Partial<InvoicesBackfill>) {
        await getRepo(this.env).update("invoices_backfill", { ...data, updatedAt: new Date().toISOString() }, { ownerId });
    }

    /// KSeF published limits for metadata queries: 8/sec, 16/min, and 20/hour
    private delay(): WorkflowDelayFunction {
        return ({ctx, error}) => String(error).includes("429") ? "1 hour" : `${10 * 2 ** (ctx.attempt - 1)} seconds`;
    }
}
