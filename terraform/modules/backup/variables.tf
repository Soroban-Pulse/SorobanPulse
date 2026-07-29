# =============================================================================
# Module: backup — Variables (Issue #833)
# =============================================================================

variable "name_prefix" {
  description = "Prefix applied to every resource name."
  type        = string
}

variable "retention_days" {
  description = "Number of days to retain backups before expiration."
  type        = number
  default     = 90
}

variable "version_retention_days" {
  description = "Number of days to retain non-current object versions."
  type        = number
  default     = 30
}

variable "force_destroy" {
  description = "Allow the bucket to be destroyed even if it contains objects. Set to false in production."
  type        = bool
  default     = false
}
