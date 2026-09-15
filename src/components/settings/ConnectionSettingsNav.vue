<script setup lang="ts">
import { useRoute, useRouter } from "vue-router";
import SegmentedControl from "../ui/SegmentedControl.vue";
const route = useRoute();
const router = useRouter();
const items = [{ value: "/settings/gateway", label: "Global Gateway" }, { value: "/settings/frp", label: "FRP" }, { value: "/settings/software", label: "软件" }];
</script>

<template>
  <div class="mb-5"><SegmentedControl :items="items" :model-value="route.path" @update:model-value="router.push($event)" /></div>
</template>
