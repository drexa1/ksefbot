export const getOpenApiSpec = () => ({
    openapi: "3.0.0",
    info: {
        title: "ksefbot-api",
        version: "1.0.0"
    },
    components: {
        securitySchemes: {
            ApiKeyAuth: {
                type: "apiKey",
                in: "header",
                name: "X-API-Key",
                description: "Clients can authenticate using this header."
            }
        },
        schemas: {
            KSeFInvoice: {
                type: "object",
                properties: {
                    header: {
                        type: "object",
                        properties: {
                            formCode: {
                                type: "object",
                                properties: {
                                    value: { type: "string" },
                                    systemCode: { type: "string" },
                                    schemaVersion: { type: "string" }
                                }
                            },
                            formVariant: { type: "integer" },
                            invoiceCreationDate: { type: "string" },
                            systemInfo: { type: "string" }
                        }
                    },
                    type: {
                        type: "string",
                        enum: ["sales", "purchase"]
                    },
                    customerId: {
                        type: ["string", "null"]
                    },
                    invoiceBody: {
                        type: "object",
                        properties: {
                            currencyCode: {
                                type: "string"
                            },
                            issueDate: {
                                type: "string"
                            },
                            issueLocation: {
                                type: "string"
                            },
                            invoiceNumber: {
                                type: "string"
                            },
                            serviceDate: {
                                type: "string"
                            },
                            totalNetAmount: {
                                type: "number"
                            },
                            totalVatAmount: {
                                type: "number"
                            },
                            totalGrossAmount: {
                                type: "number"
                            },
                            annotations: {
                                type: "object",
                                properties: {
                                    cashAccounting: { type: "integer" },
                                    selfBilling: { type: "integer" },
                                    reverseCharge: { type: "integer" },
                                    splitPayment: { type: "integer" },
                                    exemption: {
                                        type: "object",
                                        properties: {
                                            vatExemption: { type: "integer" }
                                        }
                                    },
                                    newMeansOfTransport: {
                                        type: "object",
                                        properties: {
                                            newTransport: { type: "integer" }
                                        }
                                    },
                                    specialVatTransaction: { type: "integer" },
                                    marginScheme: {
                                        type: "object",
                                        properties: {
                                            marginSchemeIndicator: { type: "integer" }
                                        }
                                    }
                                }
                            },
                            invoiceType: { type: "string" },
                            invoiceLines: {
                                type: "array",
                                items: {
                                    type: "object",
                                    properties: {
                                        lineNumber: { type: "integer" },
                                        itemDescription: { type: "string" },
                                        unitOfMeasure: { type: "string" },
                                        quantity: { type: "number" },
                                        unitPriceNet: { type: "number" },
                                        lineNetValue: { type: "number" },
                                        lineVatAmount: { type: "number" },
                                        vatRate: { type: "number" }
                                    }
                                }
                            },
                            payment: {
                                type: "object",
                                properties: {
                                    paymentDueDate: {
                                        type: "object",
                                        properties: {
                                            dueDate: { type: "string" }
                                        }
                                    },
                                    paymentMethod: { type: "integer" },
                                    bankAccount: {
                                        type: "object",
                                        properties: {
                                            accountNumber: { type: "string" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            User: {
                type: "object",
                additionalProperties: false,
                required: [
                    "tier"
                ],
                properties: {
                    email: {
                        type: "string",
                        format: "email",
                        description: "Email address provided during account creation."
                    },
                    googleSubject: {
                        type: "string",
                        description: "Google account subject from SSO login."
                    },
                    phone: {
                        type: "string",
                        description: "Phone number in E.164 format."
                    },
                    companyLogo: {
                        type: "string",
                        format: "byte",
                        description: "Encoded company logo."
                    },
                    contractorId: {
                        type: "string",
                        description: "Related contractor data."
                    },
                    tier: {
                        type: "integer",
                        description: "Application tier."
                    },
                    apiKey: {
                        type: "string",
                        description: "API key granted to this user."
                    },
                    ksefApiToken: {
                        type: "string",
                        description: "KSeF API token for KSeF integration."
                    },
                    defaultItemName: {
                        type: "string",
                        description: "Default invoice item name."
                    },
                    defaultHourlyRate: {
                        type: "integer",
                        description: "Default hourly rate."
                    },
                    settlementType: {
                        type: "string",
                        enum: ["monthly", "quarterly"],
                        description: "Settlement frequency."
                    },
                    bankName: {
                        type: "string",
                        description: "Bank name."
                    },
                    bankAccountNumber: {
                        type: "string",
                        description: "Bank account number."
                    },
                    bankApiToken: {
                        type: "string",
                        description: "Banking integration API token."
                    },
                    createdAt: {
                        type: "string",
                        format: "date-time"
                    },
                    updatedAt: {
                        type: "string",
                        format: "date-time"
                    }
                }
            },			
            Contractor: {
                type: "object",
                additionalProperties: false,
                required: [
                    "name",
                    "addressL1"
                ],
                properties: {
                    name: {
                        type: "string",
                        description: "Full user name as shown on the invoices."
                    },
                    nip: {
                        type: "string",
                        description: "PL tax payer identifier."
                    },
                    pesel: {
                        type: "string",
                        description: "PL national identifier."
                    },
                    regon: {
                        type: "string",
                        description: "PL unique business identifier."
                    },
                    internalIdentifier: {
                        type: "string",
                        description: "In case none of the other identifiers are available."
                    },
                    addressL1: {
                        type: "string",
                        description: "Primary address.",
                    },
                    addressL2: {
                        type: "string",
                        description: "Optional secondary address.",
                    },
                    countryCode: {
                        type: "string",
                        default: "PL",
                        description: "Two letter country code.",
                    },
                    localGovernmentUnit: {
                        type: "integer",
                        description: "(JST) 0: NA, 1: municipality, 2: county, 3: voivodeship.",
                    },
                    vatGroup: {
                        type: "integer",
                        description: "(GV) 1: invoice concerns a VAT group member, 2: invoice does not concern a VAT group member.",
                    },
                    notes: {
                        type: "string"
                    }
                }
            },
            TaxRecord: {
                type: "object",
                additionalProperties: false,
                required: [
                    "from",
                    "to",
                    "brutIncome"
                ],
                properties: {
                    from: {
                        type: "string",
                        format: "date",
                        description: "Start date of the reporting period.",
                        example: "2026-07-01"
                    },
                    to: {
                        type: "string",
                        format: "date",
                        description: "End date of the reporting period.",
                        example: "2026-07-31"
                    },
                    brutIncome: {
                        type: "number",
                        description: "Brut income. Typically the hourly rate × number of hours",
                    },
                    vatPercentage: {
                        type: "number",
                        default: 23,
                        description: "By default standard VAT rate of 23%."
                    },
                    taxRate: {
                        type: "number",
                        default: 12,
                        description: "By default standard flat rate of 12%."
                    },
                    healthInsuranceBase: {
                        type: "number",
                        default: 5537.18,
                        description: "Statutory monthly base = 60% × average monthly salary."
                    },
                    healthInsuranceRate: {
                        type: "number",
                        default: 9,
                        description: "PL health insurance law: 9% of the contribution base."
                    },
                    purchaseSummary: {
                        type: "array",
                        items: {
                            $ref: "#/components/schemas/PurchasesSummary"
                        }
                    },
                    totalCleanRevenue: {
                        type: "number",
                        description: "Clean revenue after obligations"
                    },
                    notes: {
                        type: "string"
                    }
                }
            },
            PurchasesSummary: {
                type: "object",
                properties: {
                    InvoiceNumber: { type: "string" },
                    TotalGrossAmount: { type: "number" },
                    TotalVatAmount: { type: "number" }
                }
            }
        }
    },
    tags: [
        { name: "Health" },
        { name: "Auth" },
        { name: "KSeF" },
        { name: "Users" },
        { name: "Invoices" },
        { name: "Contractors" },
        { name: "Taxes" },
    ],
    paths: {
        "/health": {
            get: {
                summary: "Health check",
                tags: ["Health"],
                responses: {
                    "200": { description: "OK" }
                }
            }
        },
        "/whoami": {
            get: {
                summary: "Get current authenticated user",
                tags: ["Auth"],
                security: [{ ApiKeyAuth: [] }],
                responses: {
                    "200": {
                        description: "Current user info",
                        content: {
                            "application/json": {
                                schema: {
                                    type: "object",
                                    properties: {
                                        user: {
                                            type: "object",
                                            properties: {
                                                name: { type: "string" },
                                                email: { type: "string" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },
                    "401": { description: "Unauthorized" },
                    "500": { description: "Failed to decode JWT" }
                }
            }
        },
        "/ksef/sales": {
            get: {
                summary: "List sales invoices at KSeF - Restricted to resources owned by the authenticated user.",
                tags: ["KSeF"],
                security: [{ ApiKeyAuth: [] }],
                parameters: [
                    {
                        name: "from",
                        in: "query",
                        required: true,
                        description: "Start date",
                        schema: { type: "string", format: "date", example: "2026-07-01" }
                    }, {
                        name: "to",
                        in: "query",
                        required: true,
                        description: "End date",
                        schema: { type: "string", format: "date", example: "2026-08-01" }
                    }
                ],
                responses: {
                    "200": { description: "Purchase invoices" },
                    "400": { description: "Invalid date params" },
                    "401": { description: "Unauthorized" }
                }
            },
            post: {
                summary: "Submit new sales invoice to KSeF - Restricted to resources owned by the authenticated user.",
                tags: ["KSeF"],
                security: [{ ApiKeyAuth: [] }],
                requestBody: {
                    required: true,
                    content: {
                        "application/json": {
                            schema: {
                                type: "object",
                                required: ["invoice"],
                                properties: {
                                    invoice: {
                                        type: "object",
                                        description: "Invoice payload"
                                    }
                                }
                            }
                        }
                    }
                },
                responses: {
                    "200": { description: "Invoice submitted" },
                    "400": { description: "Invalid invoice" },
                    "401": { description: "Unauthorized" }
                }
            }
        },
        "/ksef/sales/sessions": {
            get: {
                summary: "Get KSeF session status.",
                tags: ["KSeF"],
                security: [{ ApiKeyAuth: [] }],
                parameters: [
                    {
                        name: "sessionReferenceNumber",
                        in: "query",
                        required: false,
                        description: "KSeF online session reference number. If omitted, the status of the current/default session is returned.",
                        schema: {
                            type: "string"
                        }
                    }
                ],
                responses: {
                    "200": { description: "KSeF session status." },
                    "400": { description: "Invalid session reference number." },
                    "401": { description: "Unauthorized." },
                    "404": { description: "KSeF session not found." },
                    "502": { description: "Failed to retrieve session status from KSeF." }
                }
            }
        },
        "/ksef/sales/status": {
            get: {
                summary: "Get KSeF sales invoice processing status.",
                tags: ["KSeF"],
                security: [{ ApiKeyAuth: [] }],
                parameters: [
                    {
                        name: "sessionReferenceNumber",
                        in: "query",
                        required: true,
                        description: "KSeF online session reference number.",
                        schema: { type: "string" }
                    },
                    {
                        name: "invoiceReferenceNumber",
                        in: "query",
                        required: true,
                        description: "KSeF invoice submission reference number.",
                        schema: { type: "string" }
                    }
                ],
                responses: {
                    "200": { description: "KSeF invoice submission status." },
                    "400": { description: "Missing or invalid reference numbers." },
                    "401": { description: "Unauthorized." },
                    "404": { description: "Submission not found." }
                }
            }
        },
        "/ksef/sales/receipt": {
            get: {
                summary: "Download KSeF sales invoice submission receipt.",
                tags: ["KSeF"],
                security: [{ ApiKeyAuth: [] }],
                parameters: [
                    {
                        name: "sessionReferenceNumber",
                        in: "query",
                        required: true,
                        description: "KSeF online session reference number.",
                        schema: { type: "string" }
                    },
                    {
                        name: "invoiceReferenceNumber",
                        in: "query",
                        required: true,
                        description: "KSeF invoice submission reference number.",
                        schema: { type: "string" }
                    },
                    {
                        name: "format",
                        in: "query",
                        required: false,
                        description: "Response format.",
                        schema: { type: "string", enum: ["xml", "json"], default: "xml" }
                    }
                ],
                responses: {
                    "200": {
                        description: "KSeF invoice submission receipt."
                    },
                    "400": { description: "Missing or invalid reference numbers." },
                    "401": { description: "Unauthorized." },
                    "404": { description: "Invoice or submission receipt not found." },
                    "502": { description: "Failed to retrieve submission receipt from KSeF." }
                }
            }
        },
        "/ksef/purchases": {
            get: {
                summary: "List purchase invoices at KSeF - Restricted to resources owned by the authenticated user.",
                tags: ["KSeF"],
                security: [{ ApiKeyAuth: [] }],
                parameters: [
                    {
                        name: "from",
                        in: "query",
                        required: true,
                        description: "Start date",
                        schema: { type: "string", format: "date", example: "2026-07-01" }
                    }, {
                        name: "to",
                        in: "query",
                        required: true,
                        description: "End date",
                        schema: { type: "string", format: "date", example: "2026-08-01" }
                    }
                ],
                responses: {
                    "200": { description: "Purchase invoices" },
                    "400": { description: "Invalid date params" },
                    "401": { description: "Unauthorized" }
                }
            }
        },
        "/app/users": {
            get: {
                summary: "List users - Allowed only for admin users.",
                tags: ["Users"],
                security: [{ ApiKeyAuth: [] }],
                parameters: [{ name: "id", in: "query", required: false, schema: { type: "string" } }],
                responses: {
                    "200": { description: "User records" },
                    "401": { description: "Unauthorized" },
                    "404": { description: "User not found" }
                }
            },
            post: {
                summary: "Create a user - Allowed only for admin users.",
                tags: ["Users"],
                security: [{ ApiKeyAuth: [] }],
                requestBody: {
                    required: true,
                    content: {
                        "application/json": {
                            schema: { $ref: "#/components/schemas/User" }
                        }
                    }
                },
                responses: {
                    "200": { description: "User created" },
                    "401": { description: "Unauthorized" }
                }
            },
            put: {
                summary: "Update a user - Allowed only for admin users.",
                tags: ["Users"],
                security: [{ ApiKeyAuth: [] }],
                requestBody: {
                    required: true,
                    content: {
                        "application/json": {
                            schema: { $ref: "#/components/schemas/User" }
                        }
                    }
                },
                responses: {
                    "200": { description: "User updated" },
                    "400": { description: "Bad request" },
                    "401": { description: "Unauthorized" }
                }
            },
            delete: {
                summary: "Delete a user - Allowed only for admin users.",
                tags: ["Users"],
                security: [{ ApiKeyAuth: [] }],
                parameters: [{ name: "id", in: "query", required: true, schema: { type: "string" } }],
                responses: {
                    "200": { description: "User deleted" },
                    "401": { description: "Unauthorized" },
                    "404": { description: "User not found" }
                }
            }
        },
        "/app/invoices": {
            get: {
                summary: "List invoices - Restricted to resources owned by the authenticated user.",
                tags: ["Invoices"],
                security: [{ ApiKeyAuth: [] }],
                parameters: [{ name: "id", in: "query", required: false, schema: { type: "string" } }],
                responses: {
                    "200": { description: "Invoice records" },
                    "401": { description: "Unauthorized" },
                    "404": { description: "Invoice not found" }
                }
            },
            post: {
                summary: "Upload an invoice XML - Restricted to resources owned by the authenticated user.",
                tags: ["Invoices"],
                security: [{ ApiKeyAuth: [] }],
                requestBody: {
                    required: ["file"],
                    content: {
                        "multipart/form-data": {
                            schema: {
                                type: "object",
                                required: ["file", "type"],
                                properties: {
                                    file: {
                                        type: "string",
                                        format: "binary",
                                        description: "Invoice XML file"
                                    },
                                    type: {
                                        type: "string",
                                        enum: ["sales", "purchase"],
                                        description: "Invoice type"
                                    },
                                    notes: {
                                        type: "string",
                                        description: "Optional notes",
                                        example: "Just some notes"
                                    }
                                }
                            }
                        }
                    }
                },
                responses: {
                    "200": { description: "Invoice created" },
                    "401": { description: "Unauthorized" }
                }
            },
            put: {
                summary: "Update an invoice - Unsupported operation.",
                tags: ["Invoices"],
                security: [{ ApiKeyAuth: [] }],
                responses: {
                    "405": { description: "Method Not Allowed - Invoice updates are not supported" }
                }
            },
            delete: {
                summary: "Delete an invoice - Restricted to resources owned by the authenticated user.",
                tags: ["Invoices"],
                security: [{ ApiKeyAuth: [] }],
                parameters: [{ name: "id", in: "query", required: true, schema: { type: "string" } }],
                responses: {
                    "200": { description: "Invoice deleted" },
                    "401": { description: "Unauthorized" },
                    "404": { description: "Invoice not found" }
                }
            }
        },
        "/app/contractors": {
            get: {
                summary: "List contractors - Restricted to resources owned by the authenticated user.",
                tags: ["Contractors"],
                security: [{ ApiKeyAuth: [] }],
                parameters: [{ name: "id", in: "query", required: false, schema: { type: "string" } }],
                responses: {
                    "200": { description: "Contractor records" },
                    "401": { description: "Unauthorized" },
                    "404": { description: "Contractor not found" }
                }
            },
            post: {
                summary: "Create a contractor - Restricted to resources owned by the authenticated user.",
                tags: ["Contractors"],
                security: [{ ApiKeyAuth: [] }],
                requestBody: {
                    required: true,
                    content: {
                        "application/json": {
                            schema: { $ref: "#/components/schemas/Contractor" }
                        }
                    }
                },
                responses: {
                    "200": { description: "Contractor stored" },
                    "401": { description: "Unauthorized" }
                }
            },
            put: {
                summary: "Update a contractor - Restricted to resources owned by the authenticated user.",
                tags: ["Contractors"],
                security: [{ ApiKeyAuth: [] }],
                requestBody: {
                    required: true,
                    content: {
                        "application/json": {
                            schema: { $ref: "#/components/schemas/Contractor" }
                        }
                    }
                },
                responses: {
                    "200": { description: "Contractor updated" },
                    "400": { description: "Bad request" },
                    "401": { description: "Unauthorized" }
                }
            },
            delete: {
                summary: "Delete a contractor - Restricted to resources owned by the authenticated user.",
                tags: ["Contractors"],
                security: [{ ApiKeyAuth: [] }],
                parameters: [{ name: "id", in: "query", required: true, schema: { type: "string" } }],
                responses: {
                    "200": { description: "Contractor deleted" },
                    "401": { description: "Unauthorized" },
                    "404": { description: "Contractor not found" }
                }
            }
        },
        "/app/taxes/simulate": {
            get: {
                summary: "Simulate tax obligations and clean revenue for the current month based on amount of hours worked and user's default hourly rate.",
                tags: ["Taxes"],
                security: [{ ApiKeyAuth: [] }],
                parameters: [
                    {
                        name: "hoursWorked",
                        in: "query",
                        required: true,
                        description: "Number of hours worked for the current monthly period.",
                        schema: {
                            type: "integer",
                            minimum: 1,
                            example: 160
                        }
                    }
                ],
                responses: {
                    "200": {
                        description: "Calculated tax obligations. No tax record is persisted.",
                        content: {
                            "application/json": {
                                schema: {
                                    type: "object",
                                    properties: {
                                        hoursWorked: { type: "integer" },
                                        hourlyRate: {
                                            type: "number",
                                            description: "Authenticated user's default hourly rate."
                                        },
                                        brutIncome: {
                                            type: "number",
                                            description: "Calculated gross income: hourly rate × hours worked.",
                                        },
                                        vatPercentage: {
                                            type: "number",
                                            example: 23
                                        },
                                        vatAmount: { type: "number" },
                                        netBeforeObligations: { type: "number" },
                                        taxRate: { type: "number" },
                                        incomeTax: { type: "number" },
                                        healthInsuranceBase: { type: "number" },
                                        healthInsuranceRate: { type: "number" },
                                        healthContribution: { type: "number" },
                                        purchasesDeductions: {
                                            type: "number",
                                            description: "VAT deductions from purchase invoices found in KSeF for the current month.",
                                        },
                                        purchasesSummary: {
                                            type: "array",
                                            items: {
                                                $ref: "#/components/schemas/PurchasesSummary"
                                            }
                                        },
                                        totalCleanRevenue: {
                                            type: "number",
                                            description: "Estimated clean revenue after tax, health contribution, and purchase VAT deductions."
                                        }
                                    }
                                }
                            }
                        }
                    },
                    "400": {
                        description: "Invalid daysWorked or default hourly rate is not configured."
                    },
                    "401": {
                        description: "Unauthorized"
                    }
                }
            }
        },
        "/app/taxes": {
            get: {
                summary: "List taxes records - Restricted to resources owned by the authenticated user.",
                tags: ["Taxes"],
                security: [{ ApiKeyAuth: [] }],
                parameters: [{ name: "id", in: "query", required: false, schema: { type: "string" } }],
                responses: {
                    "200": { description: "Tax records" },
                    "401": { description: "Unauthorized" },
                    "404": { description: "Taxes record not found" }
                }
            },
            post: {
                summary: "Create a taxes record - Restricted to resources owned by the authenticated user.",
                tags: ["Taxes"],
                security: [{ ApiKeyAuth: [] }],
                requestBody: {
                    required: true,
                    content: {
                        "application/json": {
                            schema: { $ref: "#/components/schemas/TaxRecord" }
                        }
                    }
                },
                responses: {
                    "200": { description: "Taxes record created" },
                    "401": { description: "Unauthorized" }
                }
            },
            put: {
                summary: "Update a taxes record - Restricted to resources owned by the authenticated user.",
                tags: ["Taxes"],
                security: [{ ApiKeyAuth: [] }],
                requestBody: {
                    required: true,
                    content: {
                        "application/json": {
                            schema: { $ref: "#/components/schemas/TaxRecord" }
                        }
                    }
                },
                responses: {
                    "200": { description: "Tax record updated" },
                    "400": { description: "Bad request" },
                    "401": { description: "Unauthorized" }
                }
            },
            delete: {
                summary: "Delete a taxes record - Restricted to resources owned by the authenticated user.",
                tags: ["Taxes"],
                security: [{ ApiKeyAuth: [] }],
                parameters: [{ name: "id", in: "query", required: true, schema: { type: "string" } }],
                responses: {
                    "200": { description: "Taxes record deleted" },
                    "401": { description: "Unauthorized" },
                    "404": { description: "Taxes record not found" }
                }
            }
        }
    }
});

export const swaggerHtml = `
    <!DOCTYPE html>
    <html lang="en">
        <head>
            <title>ksefbot-api</title>
            <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist/swagger-ui.css"/>
        </head>
        <body>
            <div id="swagger-ui"></div>
            <script src="https://unpkg.com/swagger-ui-dist/swagger-ui-bundle.js"></script>
            <script>
                // noinspection JSUnresolvedVariable
                SwaggerUIBundle({
                    url: "/openapi.json",
                    dom_id: "#swagger-ui",
                    docExpansion: "none"
                });
            </script>
        </body>
    </html>
`;
