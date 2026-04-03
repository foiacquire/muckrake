package models

type FileTag struct {
	FileID      int64
	Tag         string
	FileHash    *string
	Fingerprint *string
}
