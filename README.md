# Moesif External Processing (ExtProc) Plugin for Solo.io Gloo Gateway

The Moesif Gloo Gateway ExtProc plugin captures API traffic from [Solo.io Gloo Gateway](https://www.solo.io/products/gloo-gateway/) and logs it to the [Moesif API Analytics and Monetization](https://www.moesif.com) platform. The implementation uses [Envoy External Processing filters](https://www.envoyproxy.io/docs/envoy/latest/configuration/http/http_filters/ext_proc_filter) and has been tested with the [Solo.io Gloo Gateway implementation of the filter](https://docs.solo.io/gateway/latest/traffic-management/extproc/about/).

- **Gloo Gateway** is a powerful ingress controller and an advanced API and AI gateway, natively supporting the Kubernetes Gateway API.
- **Moesif** is an API analytics and monetization platform.

[Source Code on GitHub](https://github.com/solo-io/moesif-gloo-extproc-plugin)

## How to Install

### 1. Follow Gloo Gateway Installation Instructions

The most up-to-date steps to deploy Solo.io Gloo Gateway can be found on this [documentation page](https://docs.solo.io/gateway/latest/quickstart/). This guide also includes the installation of the `HTTPBin` demo application, which will be used later as an example.

### 2. Deploy the Plugin and Configure Kubernetes Settings

Replace the placeholder API key with the one provided by Moesif to associate your Gloo Gateway instance. You can find this in your Moesif dashboard:

```bash 
export MOESIF_APP_ID=<API key from your Moesif dashboard>
```

Apply the following Kubernetes manifest to deploy the filter, Kubernetes deployments, services, and upstream resources for the ExtProc filter. You may want to review the `env:` section of the deployment and adjust it according to Moesif's recommendations:

```bash
kubectl apply -f- <<EOF
apiVersion: apps/v1
kind: Deployment
metadata:
  name: moesif-extproc-plugin
  namespace: gloo-system
spec:
  selector:
    matchLabels:
      app: moesif-extproc-plugin
  replicas: 1
  template:
    metadata:
      labels:
        app: moesif-extproc-plugin
    spec:
      containers:
        - name: moesif-extproc-plugin
          image: gcr.io/solo-test-236622/moesif-extproc-plugin:latest
          imagePullPolicy: Always
          ports:
            - containerPort: 50051
          env:
            - name: MOESIF_APPLICATION_ID
              value: $MOESIF_APP_ID # <YOUR APPLICATION ID HERE>
            - name: USER_ID_HEADER
              value: "X-User-Example-Header"
            - name: COMPANY_ID_HEADER
              value: "X-Company-Example-Header"
            - name: UPSTREAM
              value: "outbound|443||api.moesif.net"
            - name: DEBUG
              value: "false"
            - name: RUST_LOG
              value: info
---
apiVersion: v1
kind: Service
metadata:
  name: moesif-extproc-plugin
  namespace: gloo-system
  labels:
    app: moesif-extproc-plugin
  annotations:
    gloo.solo.io/h2_service: "true"
spec:
  ports:
  - port: 4445
    targetPort: 50051
    protocol: TCP
  selector:
    app: moesif-extproc-plugin
---
apiVersion: gloo.solo.io/v1
kind: Upstream
metadata:
  labels:
    app: moesif-extproc-plugin
    discovered_by: kubernetesplugin
  name: moesif-extproc-plugin
  namespace: gloo-system
spec:
  discoveryMetadata: {}
  useHttp2: true
  kube:
    selector:
      app: moesif-extproc-plugin
    serviceName: moesif-extproc-plugin
    serviceNamespace: gloo-system
    servicePort: 4445
EOF
```

### 3. Enable extProc in Gloo Gateway

To do that please apply the following parameters to Gloo Gateway `settings.gloo.solo.io` custom resource:

```bash
kubectl patch settings default -n gloo-system --type='merge' -p '{
  "spec": {
    "extProc": {
      "allowModeOverride": false,
      "failureModeAllow": true,
      "filterStage": {
        "stage": "AuthZStage"
      },
      "grpcService": {
        "extProcServerRef": {
          "name": "moesif-extproc-plugin",
          "namespace": "gloo-system"
        }
      },
      "processingMode": {
        "requestHeaderMode": "SEND",
        "responseHeaderMode": "SEND"
      }
    }
  }
}'
```

### 4. Test

Make a few API calls that pass through the Gloo Gateway. These calls should now be logged to your Moesif account.

## How to Use

### Capturing API traffic

The Moesif plugin for Gloo Gateway captures API traffic and logs it to Moesif automatically when Gloo Gateway routes traffic through the plugin. Gloo Gateway traffic flow is defined via [Kubernetes Gateway APIs](https://gateway-api.sigs.k8s.io/) that allows only required traffic to be accessible by the plugin.

### Identifying users and companies

This plugin will automatically identify API users so you can associate API traffic to web traffic and create cross-platform funnel reports of your customer journey. The plugin currently supports reading request headers to identify users and companies automatically from events.

- If the `user_id_header` or `company_id_header` configuration option is set, the named request header will be read from each request and its value will be included in the Moesif event model as the `user_id` or `company_id` field, respectively.

2. You can associate API users to companies for tracking account-level usage. This can be done either with the company header above or through the Moesif [update user API](https://www.moesif.com/docs/api#update-a-user) to set a `company_id` for a user. Moesif will associate the API calls automatically.

## Configuration Options

These configuration options are specified as variables in the `env:` portion of the filter Kubernetes deployment.

| Option                  | Type    | Default      | Description                                                                                                                            |
| ----------------------- | ------- | ------------ | -------------------------------------------------------------------------------------------------------------------------------------- |
| `moesif_application_id` | String  | None         | **Required.** Your Moesif Application Id. Can be found within the Moesif Portal.                                                       |
| `user_id_header`        | String  | None         | Optional. The header key for User Id. If provided, the corresponding header value is used as the User Id in Moesif event models.       |
| `company_id_header`     | String  | None         | Optional. The header key for Company Id. If provided, the corresponding header value is used as the Company Id in Moesif event models. |
| `batch_max_size`        | Integer | 100          | Optional. The maximum batch size of events to be sent to Moesif.                                                                       |
| `batch_max_wait`        | Integer | 2000         | Optional. The maximum wait time in milliseconds before a batch is sent to Moesif, regardless of the batch size.                        |
| `upstream`              | String  | "moesif_api" | Optional. The upstream cluster that points to Moesif's API.                                                                            |

## Example

Based on Gloo Gateway External process documentation, this example shows how the traffic that is sent to the `HTTPBin` service gets monitored by the Moesif platform via the ExtProc HTTP filter.

### Prerequisites

You'll need access to a Kubernetes cluster into which to install the Moesif Istio WASM Plugin.

- Kubernetes Cluster, cloud-based or hosted cluster installation should be very similar.
- [kubectl](https://kubernetes.io/docs/tasks/tools/install-kubectl/) (v1.29 or later)

### Gloo Gateway installation

For ExtProc support, you will need the enterprise version of Gloo Gateway. If not deployed yet, after obtaining your license key, follow the instructions on [this page](https://docs.solo.io/gateway/latest/quickstart/#install-gloo-gateway).

**Note**: Remember to select the `Enterprise Edition` tab.

### Deploy a sample application

At [the same page](https://docs.solo.io/gateway/latest/quickstart/#deploy-a-sample-app) you can find instructions on how to install the `HTTPBin` Kubernetes deployment and service in your Kubernetes cluster.

### Install Moesif ExtProc Plugin for Solo.io Gloo Gateway

Follow steps from the [installation instructions](#2-deploy-the-plugin-and-configure-the-required-settings-of-kubernetes-deployment) above.

## Example: Accessing HTTPBin Service via Gloo Gateway

This section shows how to interact with the `HTTPBin` service after completing the installation.

### 1. Verify the Gloo Gateway Address

Get the Kubernetes Load Balancer address associated with the `HTTPBin` service:

```bash
export INGRESS_GW_ADDRESS=$(kubectl get svc -n gloo-system gloo-proxy-http -o=jsonpath="{.status.loadBalancer.ingress[0]['hostname','ip']}")
echo $INGRESS_GW_ADDRESS
```

### 2. Test the Service

You can now send a request to the `HTTPBin` service running inside the cluster from your console:

```bash
curl -i http://$INGRESS_GW_ADDRESS:8080/headers -H "host: www.example.com:8080"
```

### Expected Response

The expected response from the service should look like this:

```output
$ curl -i http://$INGRESS_GW_ADDRESS:8080/headers -H "host: www.example.com:8080"
HTTP/1.1 200 OK
access-control-allow-credentials: true
access-control-allow-origin: *
content-type: application/json; encoding=utf-8
date: Wed, 04 Sep 2024 18:24:55 GMT
content-length: 336
x-envoy-upstream-service-time: 3
server: envoy

{
  "headers": {
    "Accept": [
      "*/*"
    ],
    "Host": [
      "www.example.com:8080"
    ],
    "User-Agent": [
      "curl/7.81.0"
    ],
    "X-Envoy-Expected-Rq-Timeout-Ms": [
      "15000"
    ],
    "X-Forwarded-Proto": [
      "http"
    ],
    "X-Request-Id": [
      "e6f63485-a519-4f33-a7ca-0721a0be187e"
    ]
  }
}
```

### 3. Check Logs on Moesif

After the configuration is applied, check your [Moesif account](https://www.moesif.com) to see the captured events and verify that the plugin is working as expected.

## Additional Integrations

For more information on other integration options, refer to the [Moesif Integration Options Documentation](https://www.moesif.com/docs/getting-started/integration-options/).
