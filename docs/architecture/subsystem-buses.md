# Subsystem: PCI / USB / ACPI

**PCI:** `bus/pci/*`, exports Pci*, enum in `high_code`.  
**USB:** `bus/usb/*`, dedicated thread model, `RegUSBDriver` exports; docs `usbapi.txt`.  
**ACPI:** `acpi/acpi.inc`, RSDP from boot/`AcpiGetRootPtr`.
