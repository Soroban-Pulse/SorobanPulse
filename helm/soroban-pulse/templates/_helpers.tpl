{{/*
Expand the name of the chart.
*/}}
{{- define "soroban-pulse.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create a default fully qualified app name.
*/}}
{{- define "soroban-pulse.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "soroban-pulse.labels" -}}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{ include "soroban-pulse.selectorLabels" . }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{/*
Selector labels
*/}}
{{- define "soroban-pulse.selectorLabels" -}}
app.kubernetes.io/name: {{ include "soroban-pulse.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
Create or reference the workload ServiceAccount.
*/}}
{{- define "soroban-pulse.serviceAccountName" -}}
{{- if .Values.serviceAccount.create }}
{{- default (include "soroban-pulse.fullname" .) .Values.serviceAccount.name }}
{{- else }}
{{- default "default" .Values.serviceAccount.name }}
{{- end }}
{{- end }}

{{/*
Selector labels for the api component (split-mode).
*/}}
{{- define "soroban-pulse.apiSelectorLabels" -}}
app.kubernetes.io/name: {{ include "soroban-pulse.name" . }}-api
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/component: api
{{- end }}

{{/*
Selector labels for the indexer component (split-mode).
*/}}
{{- define "soroban-pulse.indexerSelectorLabels" -}}
app.kubernetes.io/name: {{ include "soroban-pulse.name" . }}-indexer
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/component: indexer
{{- end }}
