import {Env} from "../../worker";
import {getInvoicesFor} from "./ksef";

/**
 * Invoices where the user is the invoicee.
 */
export async function get(req: Request, env: Env): Promise<Response> {
    return await getInvoicesFor(req, env, "Subject2");
}