package models

type TrackedFile struct {
	ID          *int64
	SHA256      string
	Fingerprint string
	MimeType    *string
	Size        *int64
	IngestedAt  string
	Provenance  *string
}
