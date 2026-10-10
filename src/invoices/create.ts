import {getCurrentLocation} from "../location";
import {ContractorUI, loadContractors} from "../api/contractors";
import {clearValidationErrors, updateFormError, validateInvoiceForm} from "./validate";
import {loadUserProfile, preconnect, whoami} from "../api/users";
import {downloadReceipt, submitInvoice} from "../api/ksef";
import {AppUser} from "../../worker-api/types/users";
import {formatted} from "../../worker-api/service/invoices";
import {invoiceFromForm} from "../api/invoices";

// ---------------------------------------------------------------------------------------------------------------------
// Invoice data
// ---------------------------------------------------------------------------------------------------------------------
async function initInvoiceData() {
    const invoiceDataSection = document.getElementById("invoiceData") as HTMLDivElement;
    const now = new Date();
    const year = now.getFullYear();
    const month = String(now.getMonth() + 1).padStart(2, "0");
    const lastMonthDay = new Date(now.getFullYear(), now.getMonth() + 1, 0).toLocaleDateString("sv-SE");
    // Invoice number
    const invoiceNumber = invoiceDataSection.querySelector("#invoiceNumber") as HTMLInputElement;
    invoiceNumber.value = `eFA/${year}/${month}/1`;
    // Date of issue
    const issueDate = invoiceDataSection.querySelector("#issueDate") as HTMLInputElement;
    issueDate.value = formatted(now);
    // Posting date
    const postingDate = invoiceDataSection.querySelector("#postingDate") as HTMLInputElement;
    postingDate.value = lastMonthDay;
    // Delivery / service date
    const deliveryDate = invoiceDataSection.querySelector("#deliveryDate") as HTMLInputElement;
    deliveryDate.value = lastMonthDay;
    // Place of issue
    const issuePlace = invoiceDataSection.querySelector("#issuePlace") as HTMLInputElement;
    try {
        const currentLocation = await getCurrentLocation();
        issuePlace.value = currentLocation.city ?? "";
    } catch (error) {
        console.warn("Current location unavailable:", error);
        issuePlace.value = "";
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Contractor data
// ---------------------------------------------------------------------------------------------------------------------
const contractorNameInput = document.getElementById("contractorName") as HTMLInputElement;
const contractorNameSuggestions = document.getElementById("contractorNameSuggestions") as HTMLDivElement;
const identifierOptions = document.querySelectorAll<HTMLInputElement>('input[name="contractorIdentifier"]');
const contractorNip = document.getElementById("contractorNip") as HTMLElement;
const contractorNipInput = document.getElementById("contractorNipInput") as HTMLInputElement;
const contractorNipSuggestions = document.getElementById("contractorNipSuggestions") as HTMLDivElement;
const contractorTown = document.getElementById("contractorTown") as HTMLInputElement;
const contractorPostalCode = document.getElementById("contractorPostalCode") as HTMLInputElement;
const contractorStreet = document.getElementById("contractorStreet") as HTMLInputElement;
const contractorBuilding = document.getElementById("contractorBuilding") as HTMLInputElement;
const contractorApartment = document.getElementById("contractorApartment") as HTMLInputElement;
const contractorMail = document.getElementById("contractorMail") as HTMLInputElement;

let userContractor: ContractorUI;
let customers: ContractorUI[] = [];

/// Prefilled values for the Contractor Data section
async function initContractorData(userProfile: AppUser) {
    const contractors = await loadContractors();
    // Contractor data for the logged user
    userContractor = contractors.find(c => c.nip === userProfile.id || c.email === userProfile.email)!;
    userContractor
        ? console.info(`Contractor data found`)
        // Should never trigger. Contractor data for the user created during onboarding flow
        : console.error(`No contractor data found for this user`);
    // Registered customers of the logged user
    customers = contractors.filter(c => c.nip !== userProfile.id && c.email !== userProfile.email);
    customers.length > 0
        ? console.info(`${customers.length} customers(s) found`)
        : console.warn(`No customers found for this user`);  // Possible
    return { userContractor, customers };
}

/// Autocomplete by contractor name ------------------------------------------------------------------------------------
let selectedContractorIndex = -1;
setupAutocompleteKeyboardNavigation(
    contractorNameInput,
    contractorNameSuggestions,
    () => selectedContractorIndex,
    (index) => {
        selectedContractorIndex = index;
    }
);

contractorNameInput.addEventListener("input", () => {
    delete invoiceForm.dataset.customerId;
    generateInvoiceButton.disabled = true;
    selectedContractorIndex = -1;
    if (contractorNameInput.value.trim().length < 3) {
        contractorNameSuggestions.style.display = "none";
        return;
    }
    searchCustomerByName(contractorNameInput.value);
});

/// Autocomplete by contractor NIP ------------------------------------------------------------------------------------
let selectedContractorNipIndex = -1;
setupAutocompleteKeyboardNavigation(
    contractorNipInput,
    contractorNipSuggestions,
    () => selectedContractorNipIndex,
    (index) => {
        selectedContractorNipIndex = index;
    }
);

contractorNipInput.addEventListener("input", () => {
    delete invoiceForm.dataset.customerId;
    generateInvoiceButton.disabled = true;
    selectedContractorNipIndex = -1;
    if (contractorNipInput.value.trim().length < 3) {
        contractorNipSuggestions.style.display = "none";
        return;
    }
    searchCustomerByNip(contractorNipInput.value);
});

function setupAutocompleteKeyboardNavigation(
    input: HTMLInputElement,
    suggestionsContainer: HTMLDivElement,
    getSelectedIndex: () => number,
    setSelectedIndex: (index: number) => void
): void {
    input.addEventListener("keydown", (event) => {
        const suggestions = Array.from(suggestionsContainer.querySelectorAll<HTMLElement>(".contractor-suggestion"));
        if (suggestionsContainer.style.display === "none" || suggestions.length === 0)
            return;
        let selectedIndex = getSelectedIndex();
        switch (event.key) {
            case "ArrowDown":
                event.preventDefault();
                selectedIndex++;
                if (selectedIndex >= suggestions.length)
                    selectedIndex = 0;
                setSelectedIndex(selectedIndex);
                suggestions.forEach((suggestion, index) => suggestion.classList.toggle("selected", index === selectedIndex));
                break;
            case "ArrowUp":
                event.preventDefault();
                selectedIndex--;
                if (selectedIndex < 0)
                    selectedIndex = suggestions.length - 1;
                setSelectedIndex(selectedIndex);
                suggestions.forEach((suggestion, index) => suggestion.classList.toggle("selected", index === selectedIndex));
                break;
            case "Enter":
                event.preventDefault();
                if (selectedIndex >= 0)
                    suggestions[selectedIndex].click();
                break;
            case "Escape":
                event.preventDefault();
                suggestionsContainer.style.display = "none";
                setSelectedIndex(-1);
                break;
        }
    });
}

function searchCustomerByName(name: string): void {
    const results = customers.filter((c) => c.name.toLowerCase().includes(name.trim().toLowerCase()));
    renderCustomerSuggestions(contractorNameSuggestions, results, fillCustomer);
}

function searchCustomerByNip(nip: string): void {
    const results = customers.filter((c) => c.nip?.startsWith(nip.trim()));
    renderCustomerSuggestions(contractorNipSuggestions, results, fillCustomer);
}

function renderCustomerSuggestions(container: HTMLDivElement, contractors: ContractorUI[], onSelect: (contractor: ContractorUI) => void): void {
    container.innerHTML = "";
    for (const contractor of contractors) {
        const item = document.createElement("div");
        item.className = "contractor-suggestion";
        item.innerHTML = `
            <div class="form-label fw-bold">
                ${contractor.name}
            </div>
            <div class="contractor-suggestion-details">
                NIP: ${contractor.nip ?? ""}
            </div>
        `;
        item.addEventListener("click", () => {
            onSelect(contractor);
            container.style.display = "none";
        });
        container.appendChild(item);
    }
    container.style.display = contractors.length > 0 ? "block" : "none";
}

function fillCustomer(contractor: ContractorUI): void {
    invoiceForm.dataset.customerId = contractor.id;
    generateInvoiceButton.disabled = false;
    contractorNameInput.value = contractor.name;
    contractorNipInput.value = contractor.nip ?? "";
    contractorTown.value = contractor.town ?? "";
    contractorPostalCode.value = contractor.postalCode ?? "";
    contractorStreet.value = contractor.street ?? "";
    contractorBuilding.value = contractor.building ?? "";
    contractorApartment.value = contractor.apartment ?? "";
    contractorMail.value = contractor.email ?? "";
    [
        contractorNameInput,
        contractorNipInput,
        contractorTown,
        contractorPostalCode,
        contractorStreet,
        contractorBuilding,
        contractorApartment,
        contractorMail
    ].forEach((field) => {
        if (field.value.trim())
            field.classList.remove("is-invalid");
    });

    if (invoiceForm)
        updateFormError(invoiceForm);
}

/// Show/hide NIP
identifierOptions.forEach((option) => {
    option.addEventListener("change", () => {
        const showNip = option.value === "nip";
        contractorNip.style.display = showNip ? "" : "none";
        contractorNipInput.required = showNip;
        if (!showNip) contractorNipInput.value = "";
    });
});

// ---------------------------------------------------------------------------------------------------------------------
// Positions
// ---------------------------------------------------------------------------------------------------------------------

/// Recalculate net, VAT and gross per on change of price, quantity or VAT
function initPositions(userProfile: AppUser): void {
    const firstRow = document.querySelector<HTMLElement>(".item-row");
    if (firstRow) {
        const itemName = firstRow.querySelector<HTMLInputElement>('input[id^="itemName"]');
        const itemPrice = firstRow.querySelector<HTMLInputElement>('input[id^="itemPrice"]');
        if (itemName && userProfile.defaultItemName) itemName.value = userProfile.defaultItemName;
        if (itemPrice && userProfile.defaultHourlyRate) itemPrice.value = String(userProfile.defaultHourlyRate);
        calculatePositionLine(firstRow);
    }
    document.addEventListener("input", (event: Event) => {
        const target = event.target as HTMLElement;
        if (
            target.id.startsWith("itemPrice") ||
            target.id.startsWith("itemQuantity") ||
            target.id.startsWith("itemVAT")
        ) {
            const row = target.closest(".item-row");
            if (row)
                calculatePositionLine(row);
        }
    });
    document.querySelectorAll<HTMLInputElement>('input[name="priceType"]').forEach(input => {
        input.addEventListener("change", () => document.querySelectorAll(".item-row").forEach(calculatePositionLine));
    });
    initAddPosition();
    document.querySelectorAll(".item-row").forEach((row) => calculatePositionLine(row));
}

/// Calculate net, VAT and gross per invoice position
function calculatePositionLine(row: Element): void {
    const itemPrice = row.querySelector<HTMLInputElement>('input[id^="itemPrice"]')!;
    const itemQuantity = row.querySelector<HTMLInputElement>('input[id^="itemQuantity"]')!;
    const itemVATrate = row.querySelector('select[id^="itemVAT"]')! as unknown as HTMLSelectElement;

    const netInput = row.querySelector<HTMLInputElement>('input[id^="itemNet"]')!;
    const VATInput = row.querySelector<HTMLInputElement>('input[id^="itemVATamount"]')!;
    const grossInput = row.querySelector<HTMLInputElement>('input[id^="itemGross"]')!;

    const unitPrice = parseFloat(itemPrice.value) || 0;
    const quantity = parseFloat(itemQuantity.value) || 0;
    const VATrate = itemVATrate.value || "23";

    const round = (amount: number): number => Math.round(amount * 100) / 100;
    const isGross = document.querySelector<HTMLInputElement>("#priceGross")!.checked;
    const rate = VATrate === "ZW" ? 0 : Number(VATrate) / 100;
    const amount = unitPrice * quantity;
    const net = round(isGross ? amount / (1 + rate) : amount);
    const VAT = isGross ? round(round(amount) - net) : round(amount * rate);
    const gross = round(net + VAT);

    netInput.value = net.toFixed(2);
    VATInput.value = VAT.toFixed(2);
    grossInput.value = gross.toFixed(2);

    calculatePositionsTotals();
}

/// Calculate totals for net, VAT and gross
function calculatePositionsTotals(): void {
    let totalNet = 0;
    let totalVAT = 0;
    let totalGross = 0;

    document.querySelectorAll<HTMLElement>(".item-row").forEach((row) => {
        const rowNet = row.querySelector<HTMLInputElement>('input[id^="itemNet"]')!;
        const rowVAT = row.querySelector<HTMLInputElement>('input[id^="itemVATamount"]')!;
        const rowGross = row.querySelector<HTMLInputElement>('input[id^="itemGross"]')!;

        totalNet += parseFloat(rowNet.value) || 0;
        totalVAT += parseFloat(rowVAT.value) || 0;
        totalGross += parseFloat(rowGross.value) || 0;
    });

    document.getElementById("totalNet")!.textContent = totalNet.toFixed(2);
    document.getElementById("totalVAT")!.textContent = totalVAT.toFixed(2);
    document.getElementById("totalGross")!.textContent = totalGross.toFixed(2);
}

/// Add position handler
function initAddPosition() {
    document.getElementById("addItem")!.addEventListener("click", () => {
        const tbody = document.getElementById("itemsBody")!;
        const firstRow = tbody.querySelector(".item-row")!;
        const newRow = firstRow.cloneNode(true) as HTMLElement;
        newRow.querySelectorAll<HTMLInputElement>("input").forEach((input) => {
            if (input.id.startsWith("itemQuantity"))
                input.value = "1";
            if (
                input.id.startsWith("itemNet") ||
                input.id.startsWith("itemVATamount") ||
                input.id.startsWith("itemGross")
            )
                input.value = "";
        });
        tbody.appendChild(newRow);
        updateItemNumber();
        calculatePositionLine(newRow);
    });
}

/// Remove position handler
document.addEventListener("click", (event: Event) => {
    const target = event.target as HTMLElement;
    const removeButton = target.closest<HTMLButtonElement>('[id^="removePosition"]');
    if (!removeButton)
        return;
    if (document.querySelectorAll(".item-row").length <= 1)
        return;
    removeButton.closest(".item-row")?.remove();
    updateItemNumber();
    calculatePositionsTotals();
});

function updateItemNumber(): void {
    document.querySelectorAll<HTMLElement>(".item-row").forEach((row, index) => {
        const newIndex = String(index + 1);
        const rowIndex = row.querySelector<HTMLSpanElement>("#rowIndex")!;
        rowIndex.textContent = newIndex;
        // Update id's
        row.querySelectorAll<HTMLElement>("[id]").forEach((el) => el.id = el.id.replace(/\d+$/, newIndex));
        row.querySelectorAll<HTMLLabelElement>("label[for]").forEach((l) => l.htmlFor = l.htmlFor.replace(/\d+$/, newIndex));
    });
    updateRemoveButtons();
}

function updateRemoveButtons(): void {
    const rows = document.querySelectorAll<HTMLElement>(".item-row");
    const removeButtons = document.querySelectorAll<HTMLButtonElement>('[id^="removePosition"]');
    removeButtons.forEach((button) => button.disabled = rows.length <= 1);
}

// ---------------------------------------------------------------------------------------------------------------------
// Payment
// ---------------------------------------------------------------------------------------------------------------------
const paymentTermDeadline = document.getElementById("paymentTermDeadline") as HTMLInputElement;
const bankAccount = document.getElementById("bankAccount") as HTMLInputElement;
const paymentTermDescription = document.getElementById("paymentTermDescription") as HTMLInputElement;
const deadlineFields = document.getElementById("deadlineFields") as HTMLElement;
const descriptionFields = document.getElementById("descriptionFields") as HTMLElement;
const paymentDays = document.getElementById("paymentDays") as HTMLInputElement;
const paymentDeadline = document.getElementById("paymentDeadline") as HTMLInputElement;
const postingDate = document.getElementById("postingDate") as HTMLInputElement;

function initPayment(userProfile: AppUser): void {
    if (userProfile.bankAccountNumber)
        bankAccount.value = userProfile.bankAccountNumber;
    updatePaymentDeadline();
}

function updatePaymentDeadline(): void {
    const isDeadline = paymentTermDeadline.checked;
    deadlineFields.classList.toggle("d-none", !isDeadline);
    descriptionFields.classList.toggle("d-none", isDeadline);
    if (isDeadline) {
        const date = new Date(`${postingDate.value}T00:00:00`);
        const days = parseInt(paymentDays.value, 10) || 0;
        date.setDate(date.getDate() + days);
        const year = date.getFullYear();
        const month = String(date.getMonth() + 1).padStart(2, "0");
        const day = String(date.getDate()).padStart(2, "0");
        paymentDeadline.value = `${year}-${month}-${day}`;
    }
}

paymentTermDeadline.addEventListener("change", updatePaymentDeadline);
paymentTermDescription.addEventListener("change", updatePaymentDeadline);
paymentDays.addEventListener("input", updatePaymentDeadline);
postingDate.addEventListener("change", updatePaymentDeadline);

// ---------------------------------------------------------------------------------------------------------------------
// Other information
// ---------------------------------------------------------------------------------------------------------------------
const footersContainer = document.getElementById("footersContainer") as HTMLElement;
const addFooter = document.getElementById("addFooter") as HTMLButtonElement;
const firstFooter = document.getElementById("notes") as HTMLTextAreaElement;

function updateFooterDeleteButtons(): void {
    const footers = footersContainer.querySelectorAll(".footer-field");
    footers.forEach((footer) => {
        const removeButton = footer.querySelector(".remove-footer") as HTMLElement;
        removeButton.classList.toggle("d-none", footers.length === 1);
    });
}

let footerIndex = 1;
function updateFooterCounter(textarea: HTMLTextAreaElement): void {
    const counter = textarea.parentElement?.querySelector(".notes-count") as HTMLElement;
    if (counter)
        counter.textContent = String(textarea.value.length);
}

function createFooter(): void {
    footerIndex++;
    const footer = document.createElement("div");
    footer.className = "footer-field";
    footer.dataset.footerIndex = String(footerIndex);
    footer.innerHTML = `
        <label class="form-label fw-bold" for="notes${footerIndex}">
            Invoice footer<span class="fw-normal"> (optional)</span>
        </label>
        <textarea id="notes${footerIndex}"
                  class="form-control"
                  rows="5"
                  maxlength="3500"
                  placeholder="Enter additional comments (up to 3500 characters)"></textarea>
        <div class="d-flex justify-content-between align-items-center mt-1">
            <button type="button" class="btn btn-outline-danger btn-sm remove-footer btn-sm py-0" aria-label="Delete footer">Delete</button>
            <div class="help-text text-end">
                <i class="bi bi-info-circle ps-2"></i>Field accepts up to 3500 characters
                (<span class="notes-count">0</span>/3500)
            </div>
        </div>
    `;
    footersContainer.appendChild(footer);
    const textarea = footer.querySelector("textarea") as HTMLTextAreaElement;
    textarea.addEventListener("input", () => updateFooterCounter(textarea));
    const removeButton = footer.querySelector(".remove-footer") as HTMLButtonElement;
    removeButton.addEventListener("click", () => {
        footer.remove();
        updateFooterDeleteButtons();
    });
    updateFooterDeleteButtons();
}

addFooter.addEventListener("click", createFooter);
firstFooter.addEventListener("input", () => updateFooterCounter(firstFooter));

updateFooterDeleteButtons();

// ---------------------------------------------------------------------------------------------------------------------
// Action handlers
// ---------------------------------------------------------------------------------------------------------------------
const invoiceForm = document.getElementById("invoiceForm") as HTMLFormElement;
const generateInvoiceButton = document.getElementById("generateInvoiceButton") as HTMLButtonElement;
const downloadXmlButton = document.getElementById("downloadXmlButton") as HTMLButtonElement;
const submitButton = document.getElementById("submitButton") as HTMLButtonElement;
const downloadReceiptButton = document.getElementById("downloadReceipt") as HTMLButtonElement;

async function initActions(userProfile: AppUser) {
    let invoiceXML: string;
    let submittedInvoice: { sessionReferenceNumber: string, invoiceReferenceNumber: string };

    // Action generate invoice -----------------------------------------------------------------------------------------
    generateInvoiceButton?.addEventListener("click", async() => {
        clearValidationErrors(invoiceForm);
        if (!validateInvoiceForm(invoiceForm)) return;
        generateInvoiceButton.disabled = true;
        downloadXmlButton.disabled = true;
        submitButton.disabled = true;
        try {
            invoiceXML = await invoiceFromForm(invoiceForm);
            if (downloadXmlButton) downloadXmlButton.disabled = false;
            if (submitButton) submitButton.disabled = false;
        } catch (error) {
            console.error("Unable to generate invoice XML:", error);
            const errorElement = document.getElementById("invoiceFormError")!;
            errorElement.textContent = error instanceof Error ? error.message : String(error);
            errorElement.classList.remove("d-none");
        } finally {
            generateInvoiceButton.disabled = !invoiceForm.dataset.customerId;
        }
    });
    // After everything is initialized
    generateInvoiceButton.disabled = !invoiceForm.dataset.customerId;

    invoiceForm?.addEventListener("input", (event: Event) => {
        const field = event.target as HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement;
        if (field.classList.contains("is-invalid") && field.value.trim())
            field.classList.remove("is-invalid");
        updateFormError(invoiceForm);
    });

    invoiceForm?.addEventListener("change", (event: Event) => {
        const field = event.target as HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement;
        if (field.classList.contains("is-invalid") && field.value.trim())
            field.classList.remove("is-invalid");
        updateFormError(invoiceForm);
    });

    // Action download XML ---------------------------------------------------------------------------------------------
    downloadXmlButton?.addEventListener("click", () => {
        const {month, year} = getInvoiceFilename();
        const blob = new Blob([invoiceXML], { type: "application/xml;charset=utf-8" });
        const url = URL.createObjectURL(blob);
        const link = document.createElement("a");
        link.href = url;
        link.download = `${month}-${year}.xml`;
        link.click();
        URL.revokeObjectURL(url);
    });

    // Action submit invoice -------------------------------------------------------------------------------------------
    submitButton?.addEventListener("click", async() => {
        try {
            const {month, year} = getInvoiceFilename();
            const notes = `${userProfile.id} invoice for ${month}, ${year}`;
            submittedInvoice = await submitInvoice(invoiceXML, notes);
            console.info(
                `Invoice submitted successfully: ${submittedInvoice.invoiceReferenceNumber} ` +
                `(KSeF session: ${submittedInvoice.sessionReferenceNumber})`
            );
            generateInvoiceButton.disabled = true;
            submitButton.disabled = true;
            downloadReceiptButton.disabled = false;
        } catch (error) {
            console.error("Unable to submit invoice:", error);
        }
    });

    // Action download receipt -------------------------------------------------------------------------------------------
    downloadReceiptButton?.addEventListener("click", async () => {
        try {
            if (!submittedInvoice.invoiceReferenceNumber || !submittedInvoice.sessionReferenceNumber) return;
            const status = await downloadReceipt(submittedInvoice.invoiceReferenceNumber, submittedInvoice.sessionReferenceNumber);
            const response = await fetch(status.upoDownloadUrl);
            const {month, year} = getInvoiceFilename();
            const blob = await response.blob();
            const downloadUrl = URL.createObjectURL(blob);
            const link = document.createElement("a");
            link.href = downloadUrl;
            link.download = `${month}-${year}-UPO.xml`;
            document.body.appendChild(link);
            link.click();
            link.remove();
            URL.revokeObjectURL(downloadUrl);
        } catch (error) {
            console.error("Unable to download invoice receipt:", error);
        } finally {
            downloadReceiptButton.disabled = false;
        }
    });
}

function getInvoiceFilename() {
    const now = new Date();
    const month = new Intl.DateTimeFormat("en-US", { month: "long" }).format(now);
    const year = now.getFullYear();
    return {month, year}
}

// ---------------------------------------------------------------------------------------------------------------------
// Initialize
// ---------------------------------------------------------------------------------------------------------------------

async function initNew() {
    preconnect();
    const authUser = await whoami();
    const userProfile = await loadUserProfile(authUser.userId);
    await initContractorData(userProfile);
    await initInvoiceData();
    initPositions(userProfile);
    void initPayment(userProfile);
    await initActions(userProfile);
}

void initNew();