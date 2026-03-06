{{/*
Copyright (c) 2025-present Don Johnson
Licensed under the Business Source License 1.1.
GasHammer is a registered trademark of Don Johnson.
*/}}

{{/*
Expand the name of the chart.
*/}}
{{- define "gashammer.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create a default fully qualified app name.
*/}}
{{- define "gashammer.fullname" -}}
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
Create chart name and version as used by the chart label.
*/}}
{{- define "gashammer.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "gashammer.labels" -}}
helm.sh/chart: {{ include "gashammer.chart" . }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
app.kubernetes.io/part-of: gashammer
com.gashammer.version: {{ .Chart.AppVersion | quote }}
{{- end }}

{{/*
Hive labels
*/}}
{{- define "gashammer.hive.labels" -}}
{{ include "gashammer.labels" . }}
app.kubernetes.io/name: gashammer-hive
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/component: hive
{{- end }}

{{/*
Hive selector labels
*/}}
{{- define "gashammer.hive.selectorLabels" -}}
app.kubernetes.io/name: gashammer-hive
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
Edge labels
*/}}
{{- define "gashammer.edge.labels" -}}
{{ include "gashammer.labels" . }}
app.kubernetes.io/name: gashammer-edge
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/component: edge
{{- end }}

{{/*
Edge selector labels
*/}}
{{- define "gashammer.edge.selectorLabels" -}}
app.kubernetes.io/name: gashammer-edge
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
Hive image
*/}}
{{- define "gashammer.hive.image" -}}
{{- printf "%s:%s" .Values.hive.image.repository (default .Chart.AppVersion .Values.hive.image.tag) }}
{{- end }}

{{/*
Edge image
*/}}
{{- define "gashammer.edge.image" -}}
{{- printf "%s:%s" .Values.edge.image.repository (default .Chart.AppVersion .Values.edge.image.tag) }}
{{- end }}
