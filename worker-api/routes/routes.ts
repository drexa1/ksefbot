import {Env} from "../worker";
import {swaggerHtml, getOpenApiSpec} from "../api/openapi";
import {scalarHtml} from "../api/scalar";
import {get as healthGET} from "./health/health";
import {corsHeaders, whoami as whoamiGET} from "../auth";
import {
    get as salesGET,
    post as salesPOST,
    sessions as sessionsGET,
    invoiceStatus as invoiceStatusGET,
    downloadReceipt as receiptGET
} from "./ksef/ksef-sales";
import {get as purchaseGET} from "./ksef/ksef-purchases";
import {contractors as govContractorsGET} from "../clients/contractors";
import {contractors as ceidgGET} from "../clients/contractors/ceidg";
import {contractors as krsGET} from "../clients/contractors/krs";
import {contractors as vatLbGET} from "../clients/contractors/vat-lb";
import {get as usersGET, post as usersPOST, put as usersPUT, del as usersDELETE} from "./app/users";
import {get as contractorsGET, post as contractorsPOST, put as contractorsPUT, del as contractorsDELETE} from "./app/contractors";
import {get as invoicesGET, post as invoicesPOST, put as invoicesPUT, del as invoicesDELETE} from "./app/invoices";
import {get as taxesGET, simulate as simulateGET, post as taxesPOST, put as taxesPUT, del as taxesDELETE, } from "./app/taxes";
import {AuthError} from "../types/auth";

export type Routes = Partial<Record<Method, Route>>;
export type Method = "GET" | "POST" | "PUT" | "DELETE" | "OPTIONS";
export type Route = (req: Request, env: Env) => Promise<Response>;

const withErrorHandling = (routes: Routes): Routes => {
    const routesWithAuth: Routes = {};
    for (const [method, route] of Object.entries(routes)) {
        routesWithAuth[method as Method] = async (req: Request, env: Env) => {
            try {
                return await route(req, env);
            } catch (error: unknown) {
                console.error(error);
                if (error instanceof AuthError)
                    return Response.json({ error: error.message, details: error.details }, { status: error.status, headers: corsHeaders });
                return Response.json({ error: error }, { status: 500, headers: corsHeaders });
            }
        };
    }
    return routesWithAuth;
};

export const routes: Record<string, Routes> =  {
    //🔓 Not requiring authentication
    // Don't use redirection for root, we serve the static assets
    "/swagger":             { GET: async () => new Response(swaggerHtml, { headers: { "Content-Type": "text/html" }}) },
    "/docs":                { GET: async () => new Response(scalarHtml,  { headers: { "Content-Type": "text/html" }}) },
    "/openapi.json":        { GET: async () => Response.json(getOpenApiSpec()) },
    "/health":              { GET: healthGET },
    //🔒 Requiring authentication
    "/whoami":               withErrorHandling({ GET: whoamiGET }),
    "/ksef/sales":           withErrorHandling({ GET: salesGET, POST: salesPOST }),
    "/ksef/sales/status":    withErrorHandling({ GET: invoiceStatusGET }),
    "/ksef/sales/sessions":  withErrorHandling({ GET: sessionsGET }),
    "/ksef/sales/receipt":   withErrorHandling({ GET: receiptGET }),
    "/ksef/purchases":       withErrorHandling({ GET: purchaseGET }),
    "/gov/contractors":      withErrorHandling({ GET: govContractorsGET }),
    "/gov/ceidg":            withErrorHandling({ GET: ceidgGET }),
    "/gov/krs":              withErrorHandling({ GET: krsGET }),
    "/gov/vat-lb":           withErrorHandling({ GET: vatLbGET }),
    "/app/users":            withErrorHandling({ GET: usersGET, POST: usersPOST, PUT: usersPUT, DELETE: usersDELETE }),
    "/app/contractors":      withErrorHandling({ GET: contractorsGET, POST: contractorsPOST, PUT: contractorsPUT, DELETE: contractorsDELETE }),
    "/app/invoices":         withErrorHandling({ GET: invoicesGET, POST: invoicesPOST, PUT: invoicesPUT, DELETE: invoicesDELETE }),
    "/app/taxes/simulate":   withErrorHandling({ GET: simulateGET }),
    "/app/taxes":            withErrorHandling({ GET: taxesGET, POST: taxesPOST, PUT: taxesPUT, DELETE: taxesDELETE }),
};



