export type IncomingMessage = {
    entry: {
        id: string;
        changes: {
            value: {
                messaging_product: string;
                metadata: {
                    display_phone_number: string;
                    phone_number_id: string;
                }
                contacts?: WhatsappContact[];
                messages?: WhatsappMessage[];
            }
            field: string;
        }[]
    }[]
}

interface WhatsappContact {
    profile: { name: string };
    wa_id: string;
    user_id: string;
}

interface WhatsappMessage {
    from: string;
    from_user_id: string;
    id: string;
    timestamp: string;
    type: string;
    text?: {
        body: string;
    };
}